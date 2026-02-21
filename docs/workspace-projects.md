# Struxis Workspace 项目清单与边界契约

> 目标：定义 struxis workspace 内应包含的完整 project 集合，并明确职责边界。

## Project 清单（12）

1. `market`
2. `struxis`
3. `strategy`
4. `risk`
5. `portfolio`
6. `order`
7. `broker`
8. `backtest`
9. `replay`
10. `runtime`
11. `monitor`
12. `config`

## 边界契约表

| Project | 负责（In） | 输入 | 输出 | 不负责（Out） |
|---|---|---|---|---|
| `market` | 行情接入、标准化、聚合（tick→bar） | 交易所/文件流数据 | 标准化 `Tick`/`Bar` 流 | 策略决策、下单执行 |
| `struxis` | 结构链与状态参数计算（sbar/cbar/swing/trend/keyzone/sd） | 标准化行情流 | 结构状态、KeyZone、SD、解释字段 | 下单、仓位管理 |
| `strategy` | 策略规则编排与参数消费 | `struxis` 输出、配置 | 交易意图（开平/方向/置信） | 风控最终裁决、订单落地 |
| `risk` | 风控规则评估与门控 | 策略意图、账户状态、市场状态 | 风控通过/拒绝、限制参数 | 订单路由、网关通信 |
| `portfolio` | 持仓、权益、PnL、保证金状态维护 | 成交回报、行情、费用模型 | 账户与持仓快照 | 发单、行情接入 |
| `order` | 订单状态机、路由、回报归并 | 策略意图、风控结果 | 订单请求、订单状态更新 | 券商协议细节实现 |
| `broker` | 券商/交易所适配实现 | `order` 请求 | 交易回报、委托状态、错误码 | 策略逻辑、风控逻辑 |
| `backtest` | 历史回测、仿真撮合、费用与滑点 | 历史数据、策略/风控配置 | 回测结果、性能指标、交易明细 | 实盘网关通信 |
| `replay` | 事件重放与复盘驱动 | 历史事件流/日志 | 可重演的事件序列、复盘报告 | 策略生产逻辑 |
| `runtime` | 系统编排与生命周期管理 | 各 project 实例、配置 | 运行中的数据流/任务流程 | 业务计算细节本体 |
| `monitor` | 日志、指标、告警、健康检查 | 运行事件与度量 | 可观测性信号与告警 | 策略决策、交易执行 |
| `config` | 配置加载、校验、覆盖、热更新 | 配置文件/环境变量 | 类型化配置对象 | 行情处理、交易逻辑 |

## 依赖方向（建议）

- `runtime` 依赖并编排：`market/struxis/strategy/risk/order/portfolio/broker/monitor/config`
- `strategy` 依赖：`struxis`、`config`
- `risk` 依赖：`portfolio`、`config`
- `order` 依赖：`risk`、`broker`
- `backtest` 依赖：`market/struxis/strategy/risk/order/portfolio/config`
- `replay` 依赖：`market/struxis/strategy/monitor`

## 统一规则

1. 单一职责：每个 project 只解决一个领域问题。
2. 单向依赖：禁止环依赖，跨层通过接口或事件传递。
3. 语义优先：命名与类型表达业务概念，不暴露实现细节。
4. `struxis` 直出对象：`sbar/cbar/swing/trend/keyzone/sd` 作为一级对象，不增加额外抽象父层。
5. 决策与执行语义归属 `strategy`，`struxis` 保持纯分析职责。
