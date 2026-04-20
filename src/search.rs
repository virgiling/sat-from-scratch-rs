use tracing::debug;

use crate::{
    api::{Pass, Search},
    common::Watches,
    constants::SATResult,
    kernel::Kernel,
};

/// 默认 CDCL 搜索器实现。
///
/// 其主循环为：
/// `BCP -> (冲突 ? 分析+回跳 : 决策) -> 重复`。
pub struct Searcher;

#[cfg_attr(doc, aquamarine::aquamarine)]
impl Search for Searcher {
    /// 使用双观察字（2WL）执行布尔约束传播（BCP）。
    ///
    /// # 核心状态与索引约定
    /// - `trail[..propagated]`：已经完成传播的真文字。
    /// - `trail[propagated..]`：刚变为真、但尚未处理的文字。
    /// - 监视器按“使被监视文字变假的文字”来建索引。
    ///   换言之，若子句监视 $w$，则其条目存放在 $watch(\neg w)$。
    ///
    /// 监视列表会在子句加入内核时由内部初始化逻辑自动建立。
    ///
    /// 这也解释了这里为什么用 `lit` 访问 [watches](Kernel::watches)：
    /// 当文字 $l$ 被赋为真时，所有监视 $\neg l$ 的子句都可能失效，必须立即重检。
    ///
    /// # 算法流程
    /// 1. 从 `trail` 取出一个尚未传播的真文字 $l$。
    /// 2. 用 `mem::take` 取走 `watch(l)`，避免遍历时与原桶相互干扰。
    /// 3. 对桶内每个 watcher 执行三路判断：
    ///    - 若 `blocker` 已为真，子句已满足，直接保留 watcher；
    ///    - 否则尝试在子句中寻找新的可监视文字（值不为假），若找到则迁移监视；
    ///    - 若找不到替代文字，则该子句要么成为单子句并触发蕴含赋值，要么直接冲突。
    /// 4. 将压实后的 watcher 列表写回同一个监视桶。
    ///
    /// # 关键结果分支
    /// - **迁移监视成功**：子句继续保持“两个被监视文字”的结构，无需立即赋值。
    /// - **单子句**：另一个被监视文字成为唯一可满足候选，触发强制赋值。
    /// - **冲突**：两个被监视文字都为假，且无替代文字，返回冲突供后续分析学习。
    ///
    /// ```mermaid
    /// flowchart TD
    ///     A[从 trail 取 lit] --> B[取出 watch(lit)]
    ///     B --> C{blocker 为真?}
    ///     C -->|是| K[保留 watcher]
    ///     C -->|否| D[检查子句并规范化被监视位]
    ///     D --> E{能找到新监视文字?}
    ///     E -->|是| M[迁移 watcher 到新桶]
    ///     E -->|否| F{first_lit 为假?}
    ///     F -->|是| X[记录冲突并返回 false]
    ///     F -->|否| U[first_lit 成为单子句并赋值]
    ///     K --> N[写回压实 watcher]
    ///     M --> N
    ///     U --> N
    /// ```
    ///
    /// # 示例
    /// 设子句 $C = (\neg x \lor y \lor z)$，初始监视 $(\neg x, y)$。
    ///
    /// 监视表中对应条目为：
    /// - `watch(x)` 中存放“监视 $\neg x$”的 watcher（因为 $x = -(\neg x)$）；
    /// - `watch(\neg y)` 中存放“监视 $y$”的 watcher。
    ///
    /// 若一次决策令 $x = \top$，则 `trail` 新增 `x`：
    /// - 此时 $\neg x = \bot$，子句 $C$ 变为“需要检查”；
    /// - [propagate](Self::propagate) 会处理 `lit = x` 并读取 `watch(x)`；
    /// - 之后可能迁移到监视 $z$、推出 $y$ 为单子句，或直接报告冲突。
    ///
    /// 例如当前赋值为：
    /// - `x = true`
    /// - `y = false`
    /// - `z = unassigned`
    ///
    /// 则该子句在传播时会把监视从 `¬x` 迁移到 `z`，避免整句重复扫描。
    fn propagate(&mut self, kernel: &mut Kernel) -> bool {
        // TODO Should re-write it more rusty
        while kernel.propagated < kernel.trail.len() {
            let lit = kernel.trail[kernel.propagated];
            kernel.propagated += 1;
            let mut ws = kernel.watches(lit);
            let mut i = 0usize;
            let mut j = 0usize;
            let size = ws.len();
            while i < size {
                let blocker = ws[i].blocker;
                if kernel.value(blocker) == 1 {
                    ws[j] = ws[i];
                    i += 1;
                    j += 1;
                    continue;
                }

                let clause_id = ws[i].clause_id;
                let (first_lit, clause_len) = {
                    let clause = &mut kernel.clauses[clause_id];
                    let clause_len = clause.literals().len();
                    if clause_len > 1 && clause[0] == -lit {
                        clause[0] = clause[1];
                        clause[1] = -lit;
                    }
                    (clause[0], clause_len)
                };
                let w: Watches = Watches { clause_id, blocker: first_lit };
                i += 1;
                if kernel.value(first_lit) == 1 {
                    ws[j] = w;
                    j += 1;
                    continue;
                }
                // The first two literals are watched literals.
                let mut k = 2usize;
                while k < clause_len {
                    let lit_k = kernel.clauses[clause_id][k];
                    if kernel.value(lit_k) != -1 {
                        break;
                    }
                    k += 1;
                }
                if k < clause_len {
                    let moved_watch_lit = {
                        let clause = &mut kernel.clauses[clause_id];
                        clause[1] = clause[k];
                        clause[k] = -lit;
                        clause[1]
                    };
                    kernel.add_watch(-moved_watch_lit, w);
                } else {
                    ws[j] = w;
                    j += 1;
                    if kernel.value(first_lit) == -1 {
                        while i < size {
                            ws[j] = ws[i];
                            j += 1;
                            i += 1;
                        }
                        ws.truncate(j);
                        kernel.set_watches(lit, ws);
                        kernel.conflict = Some((clause_id, first_lit));
                        return false;
                    }
                    kernel.assign(first_lit.unsigned_abs(), first_lit, Some(clause_id));
                }
            }
            ws.truncate(j);
            kernel.set_watches(lit, ws);
        }
        true
    }

    /// 选择一个变量进行决策赋值。
    ///
    /// # 流程
    /// 1. 用 VSIDS 在“未赋值变量”中选出优先级最高的变量。
    /// 2. 通过相位启发式（phase saving / target / forced）确定该变量的极性。
    /// 3. 进入新的决策层，并更新决策统计计数。
    /// 4. 将该文字作为“决策赋值”压入 trail。
    ///
    /// ```mermaid
    /// flowchart TD
    ///     A[从 VSIDS 取最高分未赋值变量] --> B{找到变量?}
    ///     B -->|是| C[按 phase 选择极性]
    ///     C --> D[决策层 +1]
    ///     D --> E[assign(reason=None)]
    ///     B -->|否| F[所有变量已赋值 -> SAT]
    /// ```
    ///
    /// 因此这里调用 [assign](Kernel::assign) 时 `reason = None`：
    /// 该赋值不是由任何子句蕴含出来的，而是一个分支点。后续冲突分析与非时序回溯
    /// 都会以这些分支点为边界进行学习和回跳。
    ///
    /// # SAT 结束条件
    /// 如果 VSIDS 找不到任何未赋值变量，说明所有变量都已被赋值，且先前传播阶段
    /// 没有导出矛盾，因此该实例可判定为 SAT。
    fn decide(&mut self, kernel: &mut Kernel) {
        let var = kernel.vsids.next_variable(&|v| kernel.assignment[v] == 0);
        if let Some(var_id) = var {
            let lit = kernel.phases.decide_phase(var_id, true);
            debug!("c deciding variable: {:?}, and assign literal: {:?}", var_id, lit);
            kernel.statistics.decisions += 1;
            kernel.level += 1;
            kernel.assign(var_id, lit, None);
        } else {
            kernel.result = SATResult::SAT;
        }
    }

    /// 冲突分析：基于 First-UIP 构造学习子句并计算回跳层级。
    ///
    /// # 对应理论
    /// 从冲突子句出发，沿 trail 逆序做归结，
    /// 直到学习子句中“当前层变量”只剩一个（即 First-UIP）。
    ///
    /// ```mermaid
    /// flowchart TD
    ///     A[冲突子句] --> B[标记文字并统计 open]
    ///     B --> C[逆序扫描 trail 找当前层已标记文字]
    ///     C --> D[将该文字作为 resolve_lit]
    ///     D --> E[open -= 1]
    ///     E --> F{open == 0?}
    ///     F -->|否| G[跳到原因子句继续归结]
    ///     G --> B
    ///     F -->|是| H[得到 First-UIP]
    ///     H --> I[构建学习子句并计算 LBD]
    ///     I --> J[确定 backtrack_level 与 learnt]
    /// ```
    ///
    /// # 主要步骤
    /// 1. 读取当前冲突子句，标记参与归结的变量并 bump VSIDS 分数。
    /// 2. 用 `open` 统计“当前层尚未消解完”的变量个数。
    /// 3. 逆序扫描 trail，不断取原因子句继续归结，直到 `open == 0`。
    /// 4. 生成学习子句，计算 LBD，并把次高层级放到位置 1 以确定回跳层。
    /// 5. 写入 `kernel.learnt`，供后续 [backtrack](Self::backtrack) 断言。
    ///
    /// # 例子
    /// 设当前 trail 末尾为：`x5@2, x6@2, x7@2`，冲突子句为 `(!x1 v !x6 v !x7)`。
    /// 归结过程会先消掉 `x7`、再消掉 `x6`，最终在当前层仅剩一个文字（First-UIP），
    /// 得到学习子句形如 `(!x1 v !x5)`，并据此回跳到次高层。
    fn analyze(&mut self, kernel: &mut Kernel) {
        let Some((mut conflict_idx, _conflict_lit)) = kernel.conflict else {
            panic!("c no conflict clause found, crashed in propagate");
        };

        kernel.statistics.conflicts += 1;
        let conflict_level = kernel.level;
        if conflict_level == 0 {
            kernel.backtrack_level = 0;
            kernel.result = SATResult::UNSAT;
            return;
        }

        kernel.lemma.clear();
        kernel.lemma.push(0);

        let var_stamp = kernel.next_mark_epoch();
        let mut bump_vars: Vec<usize> = Vec::new();
        let mut open = 0usize;
        let mut resolve_lit = 0isize;
        let mut trail_idx = kernel.trail.len();

        while open > 0 || resolve_lit == 0 {
            let clause_len = kernel.clauses[conflict_idx].literals().len();
            for i in 0..clause_len {
                let q = kernel.clauses[conflict_idx][i];
                if q == resolve_lit {
                    continue;
                }

                let var_id = q.unsigned_abs();
                let level = kernel.vars[var_id].level;
                if level == 0 || kernel.mark_at(var_id) == var_stamp {
                    continue;
                }

                kernel.set_mark_at(var_id, var_stamp);
                kernel.vsids.bump_var_score(var_id);
                bump_vars.push(var_id);
                if level == conflict_level {
                    open += 1;
                } else {
                    kernel.lemma.push(q);
                }
            }

            loop {
                trail_idx -= 1;
                let lit = kernel.trail[trail_idx];
                let var_id = lit.unsigned_abs();
                if kernel.mark_at(var_id) == var_stamp
                    && kernel.vars[var_id].level == conflict_level
                {
                    resolve_lit = lit;
                    break;
                }
            }

            let resolve_var = resolve_lit.unsigned_abs();
            kernel.set_mark_at(resolve_var, 0);
            open -= 1;
            if open == 0 {
                break;
            }
            conflict_idx = kernel.vars[resolve_var]
                .reason
                .unwrap_or_else(|| panic!("c missing reason for lit {resolve_lit}"));
        }

        kernel.lemma[0] = -resolve_lit;

        let level_stamp = kernel.next_mark_epoch();
        let mut lbd = 0u32;
        for i in 0..kernel.lemma.len() {
            let lit = kernel.lemma[i];
            let level = kernel.vars[lit.unsigned_abs()].level;
            if level > 0 && kernel.mark_at(level) != level_stamp {
                kernel.set_mark_at(level, level_stamp);
                lbd += 1;
            }
        }

        if kernel.lemma.len() == 1 {
            kernel.backtrack_level = 0;
        } else {
            let mut max_idx = 1usize;
            let mut max_level = kernel.vars[kernel.lemma[1].unsigned_abs()].level;
            for i in 2..kernel.lemma.len() {
                let level = kernel.vars[kernel.lemma[i].unsigned_abs()].level;
                if level > max_level {
                    max_level = level;
                    max_idx = i;
                }
            }
            if max_idx != 1 {
                kernel.lemma.swap(1, max_idx);
            }
            kernel.backtrack_level = max_level;
        }

        let threshold = kernel.backtrack_level.saturating_sub(1);
        for var_id in bump_vars {
            if kernel.vars[var_id].level >= threshold {
                kernel.vsids.bump_var_score(var_id);
            }
        }

        let lemma = std::mem::take(&mut kernel.lemma);
        debug!("c learned clause: {:?}", lemma);
        let first_lit = lemma[0];
        let clause_id = kernel.add_learned_clause_with_lbd(lemma, lbd);
        if kernel.clauses[clause_id].literals().len() == 1 {
            kernel.learnt = (first_lit, None);
        } else {
            kernel.learnt = (first_lit, Some(clause_id));
        }
        kernel.conflict = None;
        if kernel.statistics.conflicts % 5000 == 0 {
            kernel.vsids.bump_decay_factor();
        }
    }

    /// 非时序回溯（Backjumping）。
    ///
    /// 该步骤会撤销所有高于 `backtrack_level` 的赋值，然后立刻断言学习子句
    /// 的 UIP 文字，使搜索跳转到更有信息的位置，而不是简单回到上一层。
    ///
    /// ```mermaid
    /// flowchart TD
    ///     A[根据 backtrack_level 弹出 trail] --> B[撤销对应 assignment]
    ///     B --> C[恢复 level 与 propagated]
    ///     C --> D[断言 learnt 文字]
    ///     D --> E[进入下一轮传播]
    /// ```
    ///
    /// 例：若当前在 7 层，分析得到 `backtrack_level = 3`，
    /// 则会一次性撤销 4~7 层赋值，再在 3 层断言学习子句首文字。
    fn backtrack(&mut self, kernel: &mut Kernel) {
        debug!(
            "c backtracking to level: {}, and assign literal: {:?}",
            kernel.backtrack_level, kernel.learnt
        );
        while let Some(&lit) = kernel.trail.last() {
            let var_id = lit.unsigned_abs();
            if kernel.vars[var_id].level <= kernel.backtrack_level {
                break;
            }
            kernel.trail.pop();
            kernel.reset_value(var_id);
        }
        kernel.level = kernel.backtrack_level;
        kernel.propagated = kernel.propagated.min(kernel.trail.len());

        let (lit, clause_id) = kernel.learnt;
        kernel.assign(lit.unsigned_abs(), lit, clause_id);
    }

    /// 驱动 CDCL 主循环直到得到结果。
    ///
    /// 流程顺序：
    /// 1. 先做传播，若冲突则分析并回跳；
    /// 2. 若无冲突且已全赋值，返回 SAT；
    /// 3. 否则执行 in-process passes，再做一次决策，继续循环。
    fn search(&mut self, kernel: &mut Kernel, in_processor: &mut Vec<Box<dyn Pass>>) -> SATResult {
        while kernel.result == SATResult::UNKNOWN {
            if kernel.result == SATResult::UNSAT {
                return SATResult::UNSAT;
            } else if !self.propagate(kernel) {
                self.analyze(kernel);
                if kernel.result == SATResult::UNSAT {
                    return SATResult::UNSAT;
                }
                self.backtrack(kernel);
            } else if kernel.satisfied() {
                return SATResult::SAT;
            } else {
                for pass in in_processor.iter_mut() {
                    if pass.applying(kernel) {
                        pass.apply(kernel);
                    }
                }
                self.decide(kernel);
            }
        }
        kernel.result
    }
}
