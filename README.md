# Struxis

Struxis is evolving into a high-performance quantitative trading system.

It provides a full pipeline:

- market data ingestion (`SBar`/`CBar`)
- structure analysis (`Swing`/`Trend`/`KeyZone`)
- supply-demand scoring (`SD`, 3-layer 9-factor model)
- analysis snapshot output for strategy-layer consumption

## Requirements

- Rust toolchain (edition 2024 compatible)
- Cargo

## Quick Start

Run tests:

```bash
cargo test -q
```

Run checks:

```bash
cargo check
```

## Workspace Commands

Check full workspace:

```bash
cargo check --workspace
```

Check a single package:

```bash
cargo check -p struxis
```

Run an app:

```bash
cargo run -p live
```

Run apps with runtime mode switch (`ctp` | `binance`):

```bash
STRUXIS_MODE=ctp cargo run -p live
STRUXIS_MODE=binance cargo run -p sim
STRUXIS_MODE=binance cargo run -p research
```

Run with multi-symbol subscription (binance mode):

```bash
STRUXIS_MODE=binance STRUXIS_SYMBOLS=I2601,ETHUSDT cargo run -p live
STRUXIS_MODE=binance STRUXIS_SYMBOLS=I2601,ETHUSDT cargo run -p sim
STRUXIS_MODE=binance STRUXIS_SYMBOLS=I2601,ETHUSDT cargo run -p research
```

Tune market feed queue and overload policy:

```bash
STRUXIS_MARKET_CHANNEL_CAPACITY=8192 \
STRUXIS_MARKET_INGRESS_CAPACITY=16384 \
STRUXIS_MARKET_OVERLOAD=drop_oldest \
STRUXIS_MODE=binance cargo run -p live
```

`STRUXIS_MARKET_OVERLOAD` supports: `drop_oldest` (default), `drop_newest`.

## Project Layout

- `struxis/src/constant.rs`, `struxis/src/events.rs`, `struxis/src/logging.rs`, `struxis/src/utils.rs`: foundational modules
- `struxis/src/bar.rs`, `struxis/src/tick.rs`, `struxis/src/receiver.rs`, `struxis/src/symbol.rs`, `struxis/src/indicator.rs`: market data and symbol/indicator modules
- `struxis/src/mtc.rs`, `struxis/src/swing.rs`, `struxis/src/trend.rs`, `struxis/src/keyzone.rs`, `struxis/src/keyzone_builder.rs`, `struxis/src/keyzone_factory.rs`, `struxis/src/sd.rs`, `struxis/src/engine.rs`: analysis modules
- `strategy/src/*`: decision and strategy modules (consume `struxis` outputs)

Module layout convention:

- Prefer flat source layout under `struxis/src`.
- Do not add a `market/` directory back unless there is a clear and reused sub-boundary.
- Do not add an `analysis/` directory back unless there is a clear and reused sub-boundary.

## SD Calibration

Single timeframe config example:

- `config/sd.default.yaml`

Profile config (symbol + timeframe overlays):

- `config/sd.profile.yaml`

Supported overlay precedence:

1. `default`
2. `timeframe` (e.g. `5m`)
3. `symbol` (e.g. `I2601`)
4. `symbol_timeframe` wildcard (e.g. `*.5m`, `I2601.*`)
5. `symbol_timeframe` exact (e.g. `I2601.5m`)

## Architecture Boundary

- `struxis` only provides `infra + market + analysis` capabilities.
- `strategy` owns `DecisionEngine` and trade decision orchestration.
- `runtime/apps` compose market feeds, analysis outputs, and strategy decisions.

## Docs

- `docs/rust-migration.md`
- `docs/CONTRIBUTING-struxis.md`
- `docs/trading-system-thinking.md`
- `docs/project-understanding.md`
- `docs/naming.md`
- `docs/rust-coding-standard.md`
- `docs/workspace-projects.md`
- `docs/workspace-layout.md`
- `docs/market-readiness-v1.md`
