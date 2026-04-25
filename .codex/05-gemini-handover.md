# Gemini Handover

## 1. 当前结论

- 当前仓库主线工作重点已经从 Typst 微优化切回到 **Focused 编辑区可用性重构**。
- 真正的大方向不是继续修 `flex` 片段渲染的小毛病，而是把 Focused 编辑区逐步迁到 **GPUI 的 TextLayout / WrappedLine 驱动模型**。
- 这个方向已经开始，不是空想：
  - `crates/sola-app/src/focused_editor.rs` 已创建并在持续扩展
  - `↑/↓` 已开始优先使用 wrapped layout helper
  - `Home/End` 已优先按视觉行工作
  - 背景点击已开始优先使用 wrapped layout hit-testing

## 2. 目前做到什么程度

### 2.1 已完成并验证

- Focused editor 基础焦点模型已收敛到单一可聚焦 surface。
- 左右移动、基础输入、Backspace、Enter、Shift 选区、Undo/Redo、Auto-apply on blur 已可工作。
- caret 已改成覆盖式视觉绘制，不再明显推挤后文。
- 代码区已改成显式行保留 + 横向滚动路径，减少错误软换行。
- `focused_editor` 模块已有这些 helper：
  - `FocusedEditorStyle`
  - `shape_focused_lines`
  - `approximate_editor_wrap_width`
  - `move_cursor_vertical_visual`
  - `visual_line_ranges`
  - `visual_line_edge_offset`
  - `hit_test_visual_offset`
- 这些 helper 已通过 `sola-app` 单元测试和 workspace 全量测试。

### 2.2 只完成了一半的部分

- `↑/↓`：已经开始优先使用 wrapped layout helper，但 shell 主渲染仍不是完整的 TextLayout surface。
- `Home/End`：已经按视觉行工作。
- 背景点击：已经开始优先走 wrapped layout hit-testing。
- **字符点击**：还没有迁移到 wrapped layout，仍主要依赖旧的 `clickable_chars` 路径。
- **selection / caret / click**：还没有统一到同一套文本布局驱动绘制模型。

## 3. 接下来大方向上要做什么

### 3.1 第一优先级

完成 Focused 编辑区的 **TextLayout / WrappedLine 驱动迁移**，不要再在 `flex + span fragment` 方案上做表面修补。

这条线的目标是一次性收敛这些问题：
- 软换行下 `↑/↓` 的一致性
- 点击命中精度
- caret/selection 的真实绘制
- 代码区 / HTML 适配区错行和布局不稳

### 3.2 第二优先级

在 Focused 编辑区稳定后，再回到：
- 更细粒度 Typst 脏块重渲染
- 更真实的 inline formula 布局
- 导出流水线后续目标（PDF / 长图）

## 4. 接下来小方向上先做什么

### 4.1 Gemini 下一步建议顺序

1. 把字符点击从 `clickable_chars` 路径迁到 `hit_test_visual_offset`
2. 给 Focused 编辑区做真正的自定义 text surface：
   - 统一 shape_text
   - 统一 paint text
   - 统一 paint selection
   - 统一 paint caret
3. 再把 `↑/↓`、`Home/End`、点击、Shift 选区全部收口到这一个 surface

### 4.2 当前最应该看的文件

- `crates/sola-app/src/focused_editor.rs`
- `crates/sola-app/src/shell.rs`
- 参考：
  - `gpui/examples/input.rs`
  - `gpui/src/elements/text.rs`
  - `gpui/src/text_system/line.rs`
  - `gpui/src/text_system/line_layout.rs`

## 5. 当前不要重复做的事

- 不要再回去继续只做 Typst 微优化，除非它直接阻塞编辑器主线。
- 不要再给旧 `flex` 片段渲染路径叠更多复杂逻辑。
- 不要把更多编辑器核心状态继续堆回 `shell.rs`，优先往 `focused_editor.rs` 收。

## 6. 当前仓库状态

- 当前工作树应为干净状态再交接。
- 交接前已经持续做了：
  - `cargo fmt --all`
  - `cargo test -p sola-app`
  - `cargo test --workspace`
  - `timeout 10s cargo run`

## 7. 交接判断

如果 Gemini 接手，应把“当前主任务”理解为：

> 不是继续补丁式修现象，而是把 Focused 编辑区真正迁到 GPUI 文本布局驱动模型。
