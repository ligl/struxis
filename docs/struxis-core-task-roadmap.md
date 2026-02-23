# Struxis 核心任务路线图（长期基线）

本文件用于固化 `struxis` 的核心任务、边界、验收标准与推进顺序，作为后续实现、重构、回归测试与 AI 协作的统一记忆基线。

> 当前前提：`SBar` 输入由 CSV 直接提供，暂不讨论 `SBar` 的生成与加载逻辑。

## 0. 总目标

围绕如下主链路稳定落地并可回归验证：

`SBar -> CBar -> Fractal(Top/Bottom) -> Swing -> Trend -> KeyZone -> SD`

约束原则：
- 一次只推进一个层级；
- 每个层级先定义语义，再实现，再加回归测试；
- 上层不得绕过下层直接构造；
- 可视化表达必须服从结构语义，不反向“定义”语义。

---

## 1. 核心任务清单（唯一口径）

### 任务1：在 `SBar` 基础上生成 `CBar` 序列

**目标**
- 通过包含关系处理，形成结构化 `CBar` 序列。

**输入**
- `SBar` 序列（时间有序）。

**输出**
- `CBar` 序列（含 `sbar_start_id / sbar_end_id / high / low`）。

**验收要点**
- `CBar` 区间覆盖全部 `SBar`（无缺口、无重叠）；
- 相邻 `CBar` 不再互相包含；
- 与增量/批量处理路径结果一致。

---

### 任务2：基于 `CBar` 识别顶底分型（Top/Bottom）

**目标**
- 在 `CBar` 序列上得到稳定、可回溯的分型标签。

**输入**
- `CBar` 序列。

**输出**
- 每个 `CBar` 的 `fractal_type`（`Top/Bottom/None`）。

**验收要点**
- 分型严格满足三根规则；
- 分型确认时机明确（避免未来函数）；
- 回溯更新范围可界定。

---

### 任务3：用顶底分型生成 `Swing`

**目标**
- 将有效分型组合为方向明确的 swing 结构。

**输入**
- 带分型的 `CBar` 序列。

**输出**
- `Swing` 序列（含方向、起止 cbar/sbar、完成状态）。

**验收要点**
- `Up` swing：`Bottom -> Top`；
- `Down` swing：`Top -> Bottom`；
- `completed` swing 不允许非方向态。

---

### 任务4：用 `Swing` 生成 `Trend`

**目标**
- 将同向 swing 连接成 trend，并处理拉回/完成逻辑。

**输入**
- `Swing` 序列。

**输出**
- `Trend` 序列（含方向、起止 swing/sbar、完成状态）。

**验收要点**
- trend 方向与内部主导 swing 一致；
- trend 起止边界可追踪；
- trend 完成/切换规则可回归。

---

### 任务5：识别 `KeyZone`

**目标**
- 从 swing/trend 派生离散 keyzone 对象。

**输入**
- `Swing`、`Trend`、`SBar`。

**输出**
- `KeyZone` 集合（离散对象，不是连续带）。

**验收要点**
- 每个 keyzone 是独立区间（上下两条水平边界）；
- 来源可追踪（`Swing/Trend/...`）；
- 区间边界可由 `SBar` 影线细化并复现。

---

### 任务6：`SD` 算法设计与实现

**目标**
- 输出可解释、可配置、可稳定回归的供需评估结果。

**输入**
- 结构窗口（主要 `SBar`，可叠加 keyzone/多周期信息）。

**输出**
- `score`、`stage`、`factors`（可解释分因子）。

**验收要点**
- 同一输入结果稳定；
- 配置项影响可预测；
- 与关键结构行为（趋势、关键区）一致。

---

## 2. 执行顺序（强约束）

1) `CBar`
2) `Fractal`
3) `Swing`
4) `Trend`
5) `KeyZone`
6) `SD`

若某一步验收未通过，不进入下一步。

---

## 3. 当前推进状态（持续更新）

- [x] 任务1：`CBar` 基线回归测试已建立（见 `struxis/tests/cbar_pipeline_tests.rs`）。
- [x] 任务2：分型确认时机与回溯一致性已落地（见 `struxis/tests/fractal_pipeline_tests.rs`）。
- [x] 任务3：swing 方向与分型配对规则已落地（见 `struxis/tests/swing_pipeline_tests.rs`；分型重叠按三根整体包络计算，单测：`swing::tests::fractal_overlap_uses_full_three_cbar_envelope`、`swing::tests::fractal_overlap_distinguishes_touching_from_intersection`；状态机：`Forming/PendingReverse/Confirmed` 延迟确认已固化）。
- [x] 任务4：trend 方向/边界/切换规则已落地（见 `struxis/tests/trend_pipeline_tests.rs`）。
- [x] 任务5：keyzone 离散对象/来源追踪/边界复现规则已落地（见 `struxis/tests/keyzone_pipeline_tests.rs`）。
- [ ] 任务6：待完成 SD 结构化设计文档与实现验证。

---

## 4. AI 协作约定（记忆增强）

后续 AI/人协作必须遵守：
- 任何实现建议先映射到本文件的 6 项任务之一；
- 每次改动必须声明处于“第几步”；
- 每步至少有 1 个可自动回归的测试入口；
- 文档口径与代码口径冲突时，先修文档再改代码或同步修正。

---

## 5. 变更记录

- 2026-02-23：创建本路线图，作为 `struxis` 核心任务唯一推进基线。
