# Struxis Contribution Checklist

本清单用于约束 Struxis 的模块边界、改动流程与质量门禁，避免结构回退到跨层耦合。

Rust 代码规范请先阅读：`docs/rust-coding-standard.md`

## 1) 提交前分类（必须）

- [ ] 本次改动属于哪一层：`infra` / `market` / `analysis` / `strategy`
- [ ] 是否新增模块（文件）
- [ ] 是否涉及跨层依赖变更

## 2) 模块归属规则（必须）

- `constant.rs / events.rs / logging.rs / utils.rs`：基础设施模块（`infra`）；不得依赖业务层。
- `bar.rs / tick.rs / receiver.rs / symbol.rs / indicator.rs`（位于 `struxis/src` 顶层）：行情接入、tick/bar、symbol、indicator；仅可依赖 `infra`。
- `mtc.rs / swing.rs / trend.rs / keyzone.rs / keyzone_builder.rs / keyzone_factory.rs / sd.rs / engine.rs`（位于 `struxis/src` 顶层，`mtc/` 为其内部子模块）：结构与评估层；可依赖 `infra + market`。
- `strategy/*`（位于 `strategy` crate）：决策与策略编排；可依赖 `struxis` 分析输出。

禁止项：

- [ ] `market -> analysis` 反向依赖
- [ ] `analysis -> strategy` 反向依赖
- [ ] 用“临时公共模块”绕过分层

## 3) 接口与兼容性检查

- [ ] 对外导出（`struxis/src/lib.rs`）是否需要新增/调整
- [ ] 旧 API 是否被破坏（若破坏，是否给出迁移说明）
- [ ] 文档映射路径是否同步更新（`docs/rust-migration.md`）

## 4) 测试与验证（必须）

- [ ] `cargo check` 通过
- [ ] `cargo test` 通过
- [ ] 若改动涉及回溯逻辑：新增或更新对应回归测试
- [ ] 若改动涉及事件链路：确认 `backtrack_id` 传播语义不变

建议命令：

```bash
cargo check
cargo test -q
```

## 5) 结构类改动附加要求

若提交包含“目录重构/模块迁移”：

- [ ] 不引入行为变化（测试结果与预期一致）
- [ ] import 路径统一到当前语义分层（`constant|events|logging|utils + bar|tick|receiver|symbol|indicator + mtc|swing|trend|keyzone|sd|engine`）
- [ ] 清理旧路径兼容层（如本次目标是语义收敛）

结构约定：

- [ ] 新增 market 相关模块时，优先放在 `struxis/src` 顶层。
- [ ] 不新增 `struxis/src/market/` 目录，除非已证明存在稳定且可复用的子边界。
- [ ] 新增 analysis 相关模块时，优先放在 `struxis/src` 顶层。
- [ ] 不新增 `struxis/src/analysis/` 目录，除非已证明存在稳定且可复用的子边界。
- [ ] 不使用 `mod.rs` 作为模块入口文件。
- [ ] 若模块需要子目录，采用 `foo.rs + foo/` 结构，并在 `foo.rs` 中使用 `mod bar;` / `pub mod bar;` 让编译器按默认规则自动解析。
- [ ] 默认不使用 `#[path = ...]` 指定模块路径；仅在无法采用标准布局时才允许特例，并需在 PR 描述中说明原因。

## 6) 变更说明模板（PR 描述建议）

```text
[Layer] infra|market|analysis|strategy
[Type] feature|refactor|fix|docs
[Scope] 影响模块
[Compatibility] breaking/non-breaking
[Validation] cargo check / cargo test / 关键回归测试
```

## 7) Broker 命名规范（必须）

- `Adapter`：实现 `ExchangeAdapter`，负责 connect / subscribe / heartbeat / poll。
- `Feed`：实现 `ExchangeFeed`，负责产出标准化 bar 流（`next_bar`）。
- `Mode`：运行模式统一命名为 `RuntimeMode`。
- 环境变量：运行模式统一使用 `STRUXIS_MODE`。

禁止回退到旧术语：

- `*Source`
- `SourceMode`
- `STRUXIS_SOURCE`
- `SourceError`
