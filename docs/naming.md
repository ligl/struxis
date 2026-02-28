# Struxis 命名词典（Workspace 版）

> 目标：统一命名风格，降低长期维护成本。

## 1. 总原则

1. 单词优先：模块/包名尽量使用单个词。
2. 语义优先：使用领域名词，不使用技术后缀。
3. 少缩写：仅保留行业通用且不歧义的缩写。
4. 一致映射：目录名、crate 名、模块名、核心类型名保持同源语义。
5. 扁平优先：`sbar/cbar/swing/trend/keyzone/sd` 直接作为 `struxis` 子级，不为其额外增加抽象父层（如 `state`）。
6. 增加抽象父层时，要遵循金融量化交易领域惯例，逻辑语义要清晰。

## 2. 禁用倾向

以下命名默认不采用（除非存在明确冲突或必要性）：

- `core`
- `engine`
- `service`
- `manager`
- `util` / `utils`
- `impl`
- `base`

## 3. Workspace 命名词典（建议）

### 3.1 Crate/目录名（单词）

- `market`：行情输入、标准化、聚合、回放读取。
- `struxis`：结构链与状态对象本体（`sbar/cbar/swing/trend/keyzone/sd`）。
- `strategy`：策略接口与调用层策略实现。
- `risk`：风控规则与门控。
- `portfolio`：持仓、权益、PnL、保证金。
- `order`：订单生命周期与路由（替代 `oms`）。
- `broker`：券商/交易所适配器。
- `backtest`：历史回测与仿真撮合。
- `replay`：事件重放与回放驱动。
- `monitor`：日志、指标、告警、运行健康。
- `config`：配置加载、合并、校验、热更新。
- `runtime`：实盘/仿真运行时编排。

### 3.2 关键类型名（示例）

- `market::Feed`
- `struxis::Context`
- `struxis::SBar`
- `struxis::CBar`
- `struxis::Swing`
- `struxis::Trend`
- `struxis::KeyZone`
- `struxis::SupplyDemand`
- `strategy::Strategy`
- `risk::Rule`
- `portfolio::Book`
- `order::Order`
- `broker::Gateway`
- `backtest::Simulator`
- `replay::Player`
- `monitor::Metric`
- `config::Profile`
- `runtime::Runner`

### 3.3 Broker 术语约束（当前生效）

- `Adapter`：实现 `ExchangeAdapter`，负责连接、订阅、心跳、轮询。
- `Feed`：实现 `ExchangeFeed`，负责产出标准化 bar 流。
- `Mode`：运行模式枚举，统一使用 `RuntimeMode`。
- 环境变量：运行模式统一使用 `STRUXIS_MODE`。

避免再引入 `Source` 术语（如 `*Source`、`SourceMode`、`STRUXIS_SOURCE`、`SourceError`）。

## 4. 现有 Struxis 的映射建议

- 当前 `MultiTimeframeContext` -> 可在语义上逐步收敛为 `struxis::Context`。
- 当前 `DataReceiver` -> 可逐步收敛为 `market::Feed`。
- 当前 `Strategy` 命名已符合目标方向。

## 5. 文件级命名建议

- 优先单词：`trend.rs`、`swing.rs`、`symbol.rs`。
- 避免笼统：`utils.rs`、`constant.rs` 可逐步拆分为明确语义文件。
- 测试命名与模块对齐：`trend_tests`、`risk_rules` 这类语义化分组优先。

## 6. 快速决策规则

当一个新模块难命名时：

1. 先回答“它在业务上是什么”，不是“它怎么实现”。
2. 能用一个名词表达就不用短语。
3. 两个名字都可行时，选更接近交易领域词汇的那个。

---

当为一个函数/方法命名时：

1. 不要用单词短语堆彻，而要用有语义，干净、清晰的组合。
2. 不要太长，要尽量贴近行业用语。

一句话标准：
**看到名字就能知道职责，且不需要解释前后缀。**
