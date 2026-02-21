# Struxis Migration Status

## 当前状态（2026-02-21）

已在 `Struxis`（当前目录：`struxis`）完成项目级翻译骨架，并可编译运行：

- 核心领域模型：`bar / swing / trend / keyzone / sd`
- 运行主链路：`receiver -> mtc -> cbar -> swing -> trend -> keyzone -> sd`
- 数据接收模块：单条、批量、CSV 输入
- 事件总线：`events::Observable`
- 指标模块：`EMA / ATR / IndicatorManager`
- 符号模块：`Symbol / SymbolRegistry / SymbolLoader`
- 策略入口：位于 `strategy` crate，消费 `struxis` 分析输出
- 工具模块：`utils::IdGenerator`、`logging::init_logging`
- KeyZone 构建层：`Swing/Trend/Channel builder + factory`

当前验证结果：

- `cargo check` 通过
- `cargo test` 通过（现有测试 22/22）

本轮新增能力（2026-02-21）：

- KeyZone 行为细化：`Strong/Weak Accept`、`Strong/Weak Reject`、`Second Push`、`Breakout Failure`
- MTC 产出 `latest_keyzone_signal`，并将行为强度以 bias 形式注入 SD 评分
- SD 支持参数化校准（层权重/因子权重/阈值/keyzone bias scale）
- strategy 层联合使用 KeyZone 行为 + SD 阶段/质量动态调节开仓阈值
- 提供外部 YAML 配置加载：`struxis/config/sd.default.yaml`

## 已映射模块

- `pulao/constant.py` -> `struxis/src/constant.rs`
- `pulao/events.py` -> `struxis/src/events.rs`
- `pulao/bar/*` -> `struxis/src/bar.rs` + `struxis/src/mtc.rs`
- `pulao/swing/*` -> `struxis/src/swing.rs`
- `pulao/trend/*` -> `struxis/src/trend.rs`
- `pulao/keyzone/*` -> `struxis/src/keyzone.rs` + `keyzone_builder.rs` + `keyzone_factory.rs`
- `pulao/sd/sd.py` -> `struxis/src/sd.rs`
- `pulao/decision/decision.py` -> `strategy/src/decision.rs`
- `pulao/indicator/*` -> `struxis/src/indicator.rs`
- `pulao/symbol/*` -> `struxis/src/symbol.rs`
- `pulao/utils.py` -> `struxis/src/utils.rs`
- `pulao/logging.py` -> `struxis/src/logging.rs`
- `pulao/strategy.py` -> `strategy/src/lib.rs`

## 下一阶段（持续翻译）

当前版本是“完整模块覆盖 + 可运行主链路”版本。下一阶段将继续逐条对齐 Python 细节算法：

1. CBar 包含关系细节与回溯重算
2. Swing 完结/延续条件的完整一致性
3. Trend 特征序列与转折判定完整一致性
4. KeyZone `compute_multi_touch` 等行为细分
5. Decision 规则由模板升级为完整策略规则

## SD 校准配置（YAML）

默认样例：`struxis/config/sd.default.yaml`

symbol + timeframe 样例：`struxis/config/sd.profile.yaml`

策略侧可按周期加载：

```rust
let mut strategy = Strategy::new("I2601");
strategy.load_sd_config(Timeframe::M5, "config/sd.default.yaml")?;
strategy.load_sd_profile("config/sd.profile.yaml")?;
strategy.set_auto_reload_sd_profile(true);

// 回测循环或实盘心跳中可按需调用（仅文件修改时重载）
if strategy.reload_sd_profile_if_changed()? {
	// profile changed and reapplied
}
```

也可直接通过 `SupplyDemandConfig::from_yaml_file(...)` 加载后，调用 `mtc.set_sd_config(timeframe, config)` 注入。

`sd.profile.yaml` 支持以下层级（后者覆盖前者）：

- `default`
- `timeframe`（如 `5m`）
- `symbol`（如 `I2601`）
- `symbol_timeframe`（支持 `*.5m`、`I2601.*`、`I2601.5m`）

## 说明

你要求“不间断一路翻译完整项目”，当前已完成结构和模块层面的全覆盖翻译；后续重点是算法行为与 Python 的逐步等价。

## 模块边界约束（Struxis）

提交流程清单见：[docs/CONTRIBUTING-struxis.md](docs/CONTRIBUTING-struxis.md)

目录分层：

- `struxis/src/{constant,events,logging,utils}.rs`：基础设施层（常量、事件、日志、通用工具）。
- `struxis/src/{bar,tick,receiver,symbol,indicator}.rs`：市场数据层。
- `struxis/src/{mtc,engine,swing,trend,keyzone,keyzone_builder,keyzone_factory,sd}.rs` + `struxis/src/mtc/*`：结构与评估层。
- `strategy/src/*`：执行层（decision/strategy）。

依赖方向（必须遵守）：

- `infra`（`constant/events/logging/utils`）：不依赖其他业务层。
- `market`：可依赖 `infra`；不可依赖 `analysis` / `strategy`。
- `analysis`：可依赖 `infra` + `market`；不可依赖 `strategy`。
- `strategy`：可依赖 `struxis` 分析输出并实现决策。

工程规则：

- 新增模块时，先归属层级，再决定放置目录。
- 禁止跨层反向依赖（例如 `market -> analysis`、`analysis -> strategy`）。
- 公共数据结构优先放在 `core` 或对应领域层，避免“临时公共模块”横向扩散。
- 若必须跨层复用，优先通过上层编排而不是在下层直接引用上层。
