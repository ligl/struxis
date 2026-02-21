# Struxis Workspace 目录骨架（不迁移现有代码）

> 目标：先确定工程骨架，再逐步搬迁代码。

## 建议目录树

```text
struxis/
├── Cargo.toml                # workspace 根（后续切换）
├── market/
├── struxis/
├── strategy/
├── risk/
├── portfolio/
├── order/
├── broker/
├── backtest/
├── replay/
├── runtime/
├── monitor/
├── config/
├── apps/
│   ├── live/
│   ├── sim/
│   └── research/
└── docs/
```

## 根 Cargo.toml 模板（切换到 workspace 时使用）

```toml
[workspace]
resolver = "2"
members = [
  "market",
  "struxis",
  "strategy",
  "risk",
  "portfolio",
  "order",
  "broker",
  "backtest",
  "replay",
  "runtime",
  "monitor",
  "config",
  "apps/live",
  "apps/sim",
  "apps/research",
]
```

## 每个 crate 最小模板

```text
<name>/
├── Cargo.toml
└── src/
    └── lib.rs
```

其中 `apps/*` 使用 `main.rs`：

```text
apps/<name>/
├── Cargo.toml
└── src/
    └── main.rs
```

## 边界约束（和项目清单一致）

- `struxis` 直出一级对象：`sbar/cbar/swing/trend/keyzone/sd`
- `strategy` 消费 `struxis` 输出，不反向侵入结构计算
- `runtime` 只负责编排，不承载业务计算细节
- 禁止环依赖，跨模块通过 trait/DTO/event 交互
```
