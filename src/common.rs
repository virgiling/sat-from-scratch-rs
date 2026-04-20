/// CNF 子句、2WL watcher、VSIDS 与相位策略等公共数据结构。
use std::ops::{Index, IndexMut};

use keyed_priority_queue::KeyedPriorityQueue;
use ordered_float::OrderedFloat;

use crate::constants::{
    DEFAULT_VAR_DECAY, DEFAULT_VAR_DECAY_INC, DEFAULT_VAR_INC, INIT_PHASE, MAX_VAR_DECAY,
};

/// 求解器内部变量状态（不是 DIMACS 中变量定义本身）。
///
/// 该结构记录变量在搜索过程中的“赋值元信息”：
/// - 在哪个决策层被赋值；
/// - 在 `trail` 中的位置；
/// - 若为传播赋值，其原因子句编号。
#[derive(Debug, Clone, Copy, Default)]
pub struct Variable {
    /// 变量被赋值时所在的决策层。
    pub level: usize,
    /// 变量被压入 trail 时的位置（1-based 语义，由外部维护）。
    pub trail_index: usize,
    /// 赋值原因子句。
    ///
    /// - 决策赋值：`None`
    /// - 蕴含赋值：`Some(clause_id)`
    pub reason: Option<usize>,
}

/// 文字类型：正数表示正文字，负数表示反文字。
pub type Literal = isize;

#[derive(Debug, Clone, Default)]
/// 子句（文字析取）结构。
///
/// - `literals`：子句中的文字序列；
/// - `lbd`：学习子句质量指标（Literal Block Distance）；
/// - `garbage`：删除标记（当前版本预留位）。
pub struct Clause {
    /// 学习子句的 LBD 指标。
    lbd: u32,
    /// 子句的文字列表。
    literals: Vec<Literal>,
    /// 垃圾回收标志位。
    garbage: bool,
}

/// 双观察文字（2WL）中的 watcher 条目。
///
/// 其核心思想是：只跟踪每个子句中的少量关键文字，从而避免每次传播都整句扫描。
/// - `clause_id` 指向被监视的子句；
/// - `blocker` 通常存“另一个被监视文字”，若其已为真，则该子句已满足，可快速跳过。
///
/// ```mermaid
/// flowchart LR
///     C[子句 C 监视 l0 与 l1] --> W0[watch(-l0) 存 watcher]
///     C --> W1[watch(-l1) 存 watcher]
///     W0 --> P[当 l0 被赋假时触发检查]
///     W1 --> Q[当 l1 被赋假时触发检查]
/// ```
///
/// 例：若子句监视 `x3` 与 `-x8`，则 watcher 分别放在 `watch(-x3)` 与 `watch(x8)`。
#[cfg_attr(doc, aquamarine::aquamarine)]
#[derive(Debug, Clone, Default, Copy)]
pub struct Watches {
    pub clause_id: usize,
    pub blocker: Literal,
}

/// VSIDS 的变量活跃度表。
///
/// 启发式目标：
/// - 冲突相关变量会被 `bump`，分数上升；
/// - `var_inc` 随冲突递增并经衰减控制，强调近期冲突；
/// - 通过优先队列选取当前分数最高的未赋值变量。
///
/// 例：设初始 `var_inc = 1.0`、`var_decay = 0.8`。
/// - 第 1 次冲突命中变量 `v`：`activity[v] += 1.0`
/// - 调用 `decay_inc()` 后：`var_inc = 1.25`
/// - 第 2 次冲突再次命中 `v`：`activity[v] += 1.25`
///
/// 因而近期冲突对分数贡献更大，变量选择会更偏向“最近导致冲突”的区域。
pub struct ActivityTable {
    pq: KeyedPriorityQueue<usize, OrderedFloat<f64>>,
    activities: Vec<f64>,
    var_inc: f64,
    var_decay: f64,
}

/// 决策相位（phase）状态表。
///
/// 这是相位选择策略的简化实现：
/// - `target_phase`：偏向稳定阶段的目标相位；
/// - `saved_phases`：历史相位（phase saving），回溯后可复用。
pub struct Phases {
    /// 目标相位。
    target_phase: Vec<bool>,
    /// 保存相位（回溯后优先复用的历史相位）。
    saved_phases: Vec<bool>,
}

impl Clause {
    /// 创建空子句。
    pub fn new() -> Self {
        Self { lbd: 0, literals: Vec::new(), garbage: false }
    }

    /// 设置文字并按变量编号排序（便于规范化表示）。
    pub fn with_literals(mut self, literals: Vec<Literal>) -> Self {
        self.literals = literals;
        self.literals.sort_by_key(|l1| l1.unsigned_abs());
        self
    }

    /// 设置已按调用方规则排布的文字（不再重排）。
    pub fn with_ordered_literals(mut self, literals: Vec<Literal>) -> Self {
        self.literals = literals;
        self
    }

    /// 设置子句的 LBD 值。
    pub fn with_lbd(mut self, lbd: u32) -> Self {
        self.lbd = lbd;
        self
    }

    /// 返回子句 LBD。
    pub fn lbd(&self) -> u32 {
        self.lbd
    }

    /// 返回只读文字切片。
    pub fn literals(&self) -> &[Literal] {
        &self.literals
    }

    /// 返回该子句是否已标记为垃圾。
    pub fn garbage(&self) -> bool {
        self.garbage
    }
}

impl IntoIterator for Clause {
    type Item = Literal;
    type IntoIter = std::vec::IntoIter<Literal>;

    fn into_iter(self) -> Self::IntoIter {
        self.literals.into_iter()
    }
}

impl<'a> IntoIterator for &'a Clause {
    type Item = &'a Literal;
    type IntoIter = std::slice::Iter<'a, Literal>;

    fn into_iter(self) -> Self::IntoIter {
        self.literals.iter()
    }
}

impl Index<usize> for Clause {
    type Output = Literal;

    fn index(&self, index: usize) -> &Self::Output {
        &self.literals[index]
    }
}

impl IndexMut<usize> for Clause {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.literals[index]
    }
}

impl ActivityTable {
    /// 初始化活跃度表并把所有变量放入优先队列。
    pub fn new(max_vars: usize) -> Self {
        let mut ret = Self {
            pq: KeyedPriorityQueue::new(),
            activities: vec![0.0; max_vars + 1],
            var_inc: DEFAULT_VAR_INC,
            var_decay: DEFAULT_VAR_DECAY,
        };
        for var in 1..=max_vars {
            ret.pq.push(var, OrderedFloat(0.0));
        }
        ret
    }

    /// 提升变量活跃度（通常在冲突分析中调用）。
    ///
    /// 若变量已在堆中，更新其优先级；
    /// 若暂不在堆中，按当前活跃度重新插入。
    pub fn bump_var_score(&mut self, var_id: usize) {
        match self.pq.entry(var_id) {
            keyed_priority_queue::Entry::Occupied(e) => {
                self.activities[var_id] += self.var_inc;
                let p = OrderedFloat(self.activities[var_id]);
                e.set_priority(p);
            }
            keyed_priority_queue::Entry::Vacant(e) => {
                let p = OrderedFloat(self.activities[var_id]);
                e.set_priority(p);
            }
        }
    }

    #[inline]
    /// 增大“本轮 bump 的权重”，体现 EVSIDS 的近期偏好。
    pub fn decay_inc(&mut self) {
        self.var_inc /= self.var_decay
    }

    #[inline]
    /// 调整衰减系数，逐步降低历史冲突的影响。
    pub fn bump_decay_factor(&mut self) {
        self.var_decay += DEFAULT_VAR_DECAY_INC;
        if self.var_decay > MAX_VAR_DECAY {
            self.var_decay = MAX_VAR_DECAY;
        }
    }

    /// 选取下一个未赋值变量。
    ///
    /// `not_assigned` 由调用方提供，用于过滤当前已赋值变量。
    pub fn next_variable<F>(&mut self, not_assigned: &F) -> Option<usize>
    where
        F: Fn(usize) -> bool,
    {
        while let Some((var_id, _)) = self.pq.pop() {
            if not_assigned(var_id) {
                return Some(var_id);
            }
        }
        None
    }
}

impl Phases {
    /// 初始化相位表。
    pub fn new(max_var: usize) -> Self {
        Self {
            target_phase: vec![INIT_PHASE; max_var + 1],
            saved_phases: vec![INIT_PHASE; max_var + 1],
        }
    }

    /// 按优先级选择变量相位并返回对应文字。
    ///
    /// 当前优先级：
    /// 1. `target == true` 时使用 `target_phase`
    /// 2. 其他情况使用 `saved_phases`
    ///
    /// ```mermaid
    /// flowchart TD
    ///     A[输入 var_id, target] --> B{target?}
    ///     B -->|是| C[返回 target_phase[var_id]]
    ///     B -->|否| D[返回 saved_phases[var_id]]
    /// ```
    ///
    /// 例：若 `target=false` 且 `saved_phases[var_id]=true`，
    /// 则本次决策文字为正文字 `+var_id`。
    #[cfg_attr(doc, aquamarine::aquamarine)]
    pub fn decide_phase(&self, var_id: usize, target: bool) -> Literal {
        if target {
            return var_id as isize * (if self.target_phase[var_id] { 1 } else { -1 });
        }
        var_id as isize * (if self.saved_phases[var_id] { 1 } else { -1 })
    }

    #[inline]
    /// 保存变量最近一次赋值相位（phase saving）。
    pub fn save_phase_for_variable(&mut self, var_id: usize, phase: bool) {
        self.saved_phases[var_id] = phase;
    }
}
