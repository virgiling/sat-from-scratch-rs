use crate::{
    common::{ActivityTable, Clause, Literal, Phases, Variable, Watches},
    constants::SATResult,
};

/// 求解过程统计信息。
#[derive(Debug, Clone, Default)]
pub struct Statistics {
    /// 冲突次数。
    pub conflicts: usize,
    /// 决策次数。
    pub decisions: usize,
    /// 传播次数（预留计数位，便于后续扩展）。
    pub propagations: usize,
    /// 赋值次数。
    pub assignments: usize,
}

/// CDCL 求解核心内核状态。
///
/// `Kernel` 聚合了：
/// - CNF 子句库与 watcher 索引（2WL）；
/// - trail/决策层等搜索状态；
/// - VSIDS 与相位策略；
/// - 冲突分析与学习子句所需的临时缓存。
pub struct Kernel {
    /// 变量赋值表：`-1=false`，`0=unassigned`，`1=true`。
    pub assignment: Vec<i8>,
    /// 变量元信息表（层级、原因子句等）。
    pub vars: Vec<Variable>,
    /// 子句数据库（原始子句 + 学习子句）。
    pub clauses: Vec<Clause>,
    /// 2WL watcher 桶。
    pub watches: Vec<Vec<Watches>>,
    /// trail（仅存当前被赋为真的文字）。
    pub trail: Vec<Literal>,
    /// VSIDS 活跃度表。
    pub vsids: ActivityTable,
    /// 相位策略状态表。
    pub phases: Phases,
    /// 当前求解结果。
    pub result: SATResult,

    /// 当前决策层。
    pub level: usize,
    /// 冲突分析后要回跳到的层级。
    pub backtrack_level: usize,
    /// trail 中“已完成传播”的前缀长度。
    pub propagated: usize,
    /// 当前已赋值变量计数。
    pub assigned: usize,

    /// 当前冲突信息 `(clause_id, conflict_lit)`。
    pub conflict: Option<(usize, Literal)>,
    /// 冲突分析中的临时学习子句缓冲（lemma）。
    pub lemma: Vec<Literal>,
    /// 回跳后需要立即断言的 `(literal, reason_clause)`。
    pub learnt: (Literal, Option<usize>),

    /// 统计信息。
    pub statistics: Statistics,
    /// 冲突分析与 LBD 计算用的时间戳标记表。
    mark_table: Vec<usize>,
    /// 标记表当前 epoch。
    mark_epoch: usize,
    /// DIMACS 流式解析时暂存“尚未遇到 0 终止符”的子句。
    pending_clause: Vec<Literal>,
}

impl Kernel {
    /// 构建指定变量上限的求解内核。
    pub fn new(max_vars: usize) -> Self {
        Self {
            // 变量编号从 1 开始，索引 0 保留不用。
            assignment: vec![0; max_vars + 1],
            vars: vec![Variable::default(); max_vars + 1],
            clauses: vec![],
            watches: vec![Vec::new(); max_vars * 2],
            trail: vec![],
            vsids: ActivityTable::new(max_vars),
            phases: Phases::new(max_vars),
            result: SATResult::UNKNOWN,
            level: 0,
            backtrack_level: 0,
            propagated: 0,
            assigned: 0,
            conflict: None,
            lemma: Vec::new(),
            learnt: (0, None),
            pending_clause: Vec::new(),
            statistics: Statistics::default(),
            mark_table: vec![0; max_vars + 1],
            mark_epoch: 1,
        }
    }

    /// 以流式方式添加子句文字。
    ///
    /// - `Some(lit)`：把文字加入当前待完成子句；
    /// - `None`：结束当前子句，落库并挂接 watcher。
    pub fn add(&mut self, lit: Option<Literal>) {
        if let Some(lit) = lit {
            self.pending_clause.push(lit);
        } else {
            let literals = std::mem::take(&mut self.pending_clause);
            let clause = Clause::new().with_literals(literals).with_lbd(0);
            let clause_id = self.clauses.len();
            self.clauses.push(clause);
            self.attach_clause_watchers(clause_id);
        }
    }

    #[inline]
    /// 在当前赋值下评估文字取值。
    ///
    /// 返回值约定：
    /// - `1`：文字为真；
    /// - `-1`：文字为假；
    /// - `0`：变量尚未赋值。
    pub fn value(&self, lit: Literal) -> i8 {
        assert_ne!(lit, 0);
        assert!(lit.unsigned_abs() < self.assignment.len());
        let value = self.assignment[lit.unsigned_abs()];
        if lit > 0 { value } else { -value }
    }

    /// 把非零文字映射到 watcher 桶索引。
    ///
    /// 每个变量对应两个桶：
    /// - 正文字 `v` -> `2*(v-1)`
    /// - 反文字 `-v` -> `2*(v-1)+1`
    ///
    /// 例：`v = 3` 时，
    /// - `watch(3)` 的索引是 `4`
    /// - `watch(-3)` 的索引是 `5`
    #[inline]
    fn watcher_index(&self, lit: Literal) -> usize {
        assert_ne!(lit, 0);
        assert!(lit.unsigned_abs() < self.assignment.len());
        let var = lit.unsigned_abs();
        ((var - 1) << 1) | usize::from(lit < 0)
    }

    /// 向 `watched_lit` 对应桶追加一个 watcher。
    ///
    /// 本实现遵循 2WL 常见约定：通常存入的是 `-w`（而非 `w`），
    /// 即“让被监视文字 `w` 失效时会被触发”的桶，便于传播时 O(相关子句数) 访问。
    #[inline]
    pub fn add_watch(&mut self, watched_lit: Literal, watch: Watches) {
        let idx = self.watcher_index(watched_lit);
        self.watches[idx].push(watch);
    }

    /// 为新子句初始化 watcher。
    ///
    /// - 单子句 `(l)`：在 `watch(-l)` 挂一个 watcher；
    /// - 非单子句 `(l0 ∨ l1 ∨ ...)`：
    ///   - `l0` 的 watcher 放在 `watch(-l0)`，`blocker=l1`
    ///   - `l1` 的 watcher 放在 `watch(-l1)`，`blocker=l0`
    ///
    /// 这样传播时只需读取被新真值文字触发的桶即可。
    ///
    /// 例：子句 `(x1 ∨ -x4 ∨ x7)` 初始挂接后：
    /// - `watch(-x1)` 存一条 watcher，`blocker = -x4`
    /// - `watch(x4)` 存一条 watcher，`blocker = x1`
    fn attach_clause_watchers(&mut self, clause_id: usize) {
        let Some(clause) = self.clauses.get(clause_id) else {
            return;
        };

        let literals = clause.literals();
        assert_ne!(literals.len(), 0);
        if literals.len() == 1 {
            let lit = literals[0];
            self.add_watch(-lit, Watches { clause_id, blocker: lit });
        } else {
            let lit0 = literals[0];
            let lit1 = literals[1];
            self.add_watch(-lit0, Watches { clause_id, blocker: lit1 });
            self.add_watch(-lit1, Watches { clause_id, blocker: lit0 });
        }
    }

    /// 取走并返回 `lit` 的整桶 watcher。
    ///
    /// 传播阶段采用 “take -> 处理 -> set” 方式，避免借用冲突并允许 watcher 迁移。
    pub fn watches(&mut self, lit: Literal) -> Vec<Watches> {
        let idx = self.watcher_index(lit);
        std::mem::take(&mut self.watches[idx])
    }

    /// 将传播后压实得到的 watcher 桶写回。
    pub fn set_watches(&mut self, lit: Literal, ws: Vec<Watches>) {
        let idx = self.watcher_index(lit);
        self.watches[idx] = ws;
    }

    /// 写入一次变量赋值并记录到 trail。
    ///
    /// - `reason=None`：决策赋值；
    /// - `reason=Some(clause_id)`：传播蕴含赋值。
    ///
    /// 例：若在第 5 层写入 `lit = -9` 且 `reason = Some(12)`，
    /// 表示变量 `x9` 在第 5 层被子句 `12` 蕴含为假。
    pub fn assign(&mut self, var_id: usize, lit: Literal, reason: Option<usize>) {
        assert!(var_id > 0 && var_id < self.assignment.len());
        let mut var = self.vars[var_id];
        self.trail.push(lit);
        var.level = self.level;
        var.trail_index = self.trail.len();
        var.reason = reason;
        self.vars[var_id] = var;
        self.assignment[var_id] = if lit > 0 { 1 } else { -1 };
        self.phases.save_phase_for_variable(var_id, lit > 0);
        self.assigned += 1;
    }

    #[inline]
    /// 撤销变量赋值（用于回跳时弹栈）。
    pub fn reset_value(&mut self, var_id: usize) {
        assert!(var_id > 0 && var_id < self.assignment.len());
        self.phases.save_phase_for_variable(var_id, self.assignment[var_id] > 0);
        self.assignment[var_id] = 0;
        self.assigned -= 1;
    }

    #[inline]
    /// 申请一个新的标记 epoch。
    ///
    /// 若发生 `usize` 回绕，则清空标记表并从 1 重新开始。
    pub fn next_mark_epoch(&mut self) -> usize {
        self.mark_epoch = self.mark_epoch.wrapping_add(1);
        if self.mark_epoch == 0 {
            self.mark_epoch = 1;
            self.mark_table.fill(0);
        }
        self.mark_epoch
    }

    #[inline]
    /// 读取标记表在 `idx` 处的时间戳。
    pub fn mark_at(&self, idx: usize) -> usize {
        self.mark_table.get(idx).copied().unwrap_or(0)
    }

    #[inline]
    /// 设置标记表 `idx` 的时间戳，不足则自动扩容。
    pub fn set_mark_at(&mut self, idx: usize, stamp: usize) {
        if idx >= self.mark_table.len() {
            self.mark_table.resize(idx + 1, 0);
        }
        self.mark_table[idx] = stamp;
    }

    /// 新增学习子句并初始化 watcher，返回子句编号。
    pub fn add_learned_clause_with_lbd(&mut self, literals: Vec<Literal>, lbd: u32) -> usize {
        assert!(!literals.is_empty(), "learned clause should not be empty");
        let clause = Clause::new().with_ordered_literals(literals).with_lbd(lbd);
        let clause_id = self.clauses.len();
        self.clauses.push(clause);
        self.attach_clause_watchers(clause_id);
        clause_id
    }

    /// 判断当前是否已给所有变量赋值。
    pub fn satisfied(&self) -> bool {
        self.assigned + 1 == self.assignment.len()
    }
}
