# Struxis 项目理解（工作备忘）

> 更新时间：2026-02-21
> 目的：沉淀当前阶段对项目定位与架构的统一理解，供后续设计与实现参考。

## 1. 项目定位

Struxis 是一个 **交易分析与参数生产引擎**，不是最终策略执行器。

- 负责：接收市场数据、构建多周期结构、输出可解释信号与参数。
- 不负责：最终如何开平仓、仓位管理、资金管理、交易网关执行。
- 调用层策略负责将 Struxis 输出转化为具体交易动作。

## 2. 核心职责边界

### Struxis 内部职责（In Scope）

1. 数据输入标准化（bar/tick/csv 等）。
2. 多周期结构链维护：`SBar -> CBar -> Swing -> Trend`。
3. KeyZone 识别与行为分类（accept/reject/second push/breakout failure）。
4. SupplyDemand（SD）三层九因子评估与分阶段判定。
5. 输出给上层策略可消费的状态、分数、结构信号与解释信息。

### 调用层职责（Out of Scope for Struxis）

1. 交易动作执行（下单、撤单、风控执行）。
2. 仓位 sizing 与资金管理规则。
3. 组合级约束与跨策略调度。

## 3. 当前处理主链路（逻辑流）

1. `DataReceiver` 接收数据（bar/tick/csv）。
2. 数据进入 `MultiTimeframeContext`（按周期分治管理）。
3. 每周期内部依次更新：
   - `SBarManager`
   - `CBarManager`
   - `SwingManager`
   - `TrendManager`
4. 基于结构重建 `KeyZoneManager`，评估最新 KeyZone 行为信号。
5. 用最近窗口 + keyzone bias 计算 `SupplyDemandResult`。
6. 将结构状态与 SD 结果暴露给上层策略使用。

## 4. 设计原则（当前已体现）

1. **可解释性优先**：输出不只给结论，也给结构上下文与解释。
2. **多周期一致性**：各周期独立维护事实，跨周期做对齐约束。
3. **可回溯/可复盘**：事件通知、快照导出、确定性测试。
4. **配置可运营**：SD 支持 profile 覆盖与热加载。
5. **职责分离**：Struxis 产参数，策略层做决策与执行。

## 5. 模块理解（代码映射）

- `struxis/src/{bar,tick,receiver,symbol,indicator}.rs`：数据输入与聚合（tick -> m1 -> 高周期）。
- `struxis/src/{mtc,engine,swing,trend,keyzone,keyzone_builder,keyzone_factory,sd}.rs` + `struxis/src/mtc/*`：结构分析、关键位、供需评估、多周期上下文。
- `strategy/src/*`：策略与决策规则实现（消费 `struxis` 分析输出）。
- `struxis/src/events.rs`：分析流水线事件通知机制。
- `struxis/src/lib.rs`：对外导出统一 API。

## 6. 关于 Strategy 的说明

当前 `strategy::Strategy` 可视为“默认参考策略管线”，用于将分析结果转成 `DecisionResult`。

在项目定位上，`Strategy` 不应绑定唯一交易哲学；
后续可保留多个调用层策略实现，共享同一套 Struxis 分析输出。

## 7. 未来迭代建议（不改变定位）

1. 抽象稳定的“输出契约”（结构 + SD + KeyZone + 解释字段）。
2. 将分析输出与执行动作进一步解耦（动作可选、参数必选）。
3. 增加接口版本语义（即使暂不对外，也利于内部协作）。
4. 强化端到端回放样例，保障“相同输入 -> 相同输出”。

## 8. 一句话总结

Struxis 的核心价值是：
**把复杂市场行为压缩为可解释、可复用、可被多策略消费的统一分析参数层。**
