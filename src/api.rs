/// 此模块用于定义 pre/in-processing 技术，作为可插拔的组件
use crate::{constants::SATResult, kernel::Kernel};

/// 求解流程中的可插拔 Pass 接口。
///
/// Pass 可用于两类阶段：
/// - 预处理（preprocess）：在正式搜索前简化公式或提前判定结果；
/// - 搜索中处理（inprocess）：在 CDCL 循环中周期性介入，做轻量重启/清理/重排等。
pub trait Pass {
    /// Pass 的短名称，用于日志和排序配置。
    fn name(&self) -> &'static str;
    /// Pass 的功能描述。
    fn description(&self) -> &'static str;
    /// Pass 所属类别，例如 `preprocess` / `inprocess`。
    fn category(&self) -> &'static str;
    /// 当前内核状态下该 Pass 是否应执行。
    fn applying(&self, kernel: &Kernel) -> bool;
    /// 执行 Pass 并返回是否已能判定 SAT/UNSAT。
    fn apply(&mut self, kernel: &mut Kernel) -> SATResult;
}

/// CDCL 搜索核心接口。
///
/// 典型循环为：
/// `propagate -> (conflict ? analyze + backtrack : decide)`。
///
/// 直观伪代码：
/// `while result == UNKNOWN {`
/// `  if !propagate() { analyze(); backtrack(); }`
/// `  else if all_assigned() { return SAT; }`
/// `  else { decide(); }`
/// `}`
///
/// ```mermaid
/// flowchart TD
///     Start([开始]) --> P[propagate: BCP]
///     P -->|发现冲突| A[analyze: 冲突分析/学习]
///     A --> B[backtrack: 非时序回跳]
///     B --> P
///     P -->|无冲突且全部已赋值| Sat([返回 SAT])
///     P -->|无冲突但仍有未赋值变量| D[decide: 决策赋值]
///     D --> P
/// ```
#[cfg_attr(doc, aquamarine::aquamarine)]
pub trait Search {
    /// 执行 BCP（通常基于双观察文字）。
    ///
    /// 返回 `false` 表示传播中遇到冲突。
    fn propagate(&mut self, kernel: &mut Kernel) -> bool;
    /// 在无冲突且仍有未赋值变量时执行一次决策赋值。
    fn decide(&mut self, kernel: &mut Kernel);
    /// 对当前冲突做分析，构造学习子句并计算回跳层级。
    fn analyze(&mut self, kernel: &mut Kernel);
    /// 按 `analyze` 结果回跳，并断言学习子句中的 UIP 文字。
    fn backtrack(&mut self, kernel: &mut Kernel);
    /// 驱动完整搜索流程，直至返回 `SAT`/`UNSAT`/`UNKNOWN`。
    fn search(&mut self, kernel: &mut Kernel, in_processor: &mut Vec<Box<dyn Pass>>) -> SATResult;
}
