# GPUI 原生编辑器重构计划 - 迈向 Zed 级性能与体验

## 1. 背景与动机
当前的 Focused 编辑区采用基于 `flex + span Div` 的“片段拼接”方案。虽然在原型期能工作，但在处理长行软换行、光标精确绘制、选区性能以及交互一致性上存在天然瓶颈。

参考 **Zed** 的实现，GPUI 最正宗的编辑器实现方式是使用 **自定义 Element (`gpui::Element`)**，在 `Paint` 阶段直接操作 `TextSystem`。

## 2. 核心架构目标
- **单一 Element 驱动**：Focused Block 的文本不再由数百个 `Div` 组成，而是通过一个自定义的 `FocusedEditorElement` 一次性完成渲染。
- **TextLayout 核心**：利用 `WrappedLine` 缓存布局结果，支持高性能的软换行计算。
- **自定义绘制层**：按顺序手动绘制：
  1. 选区背景 (Selection Quads)
  2. 文本内容 (Shaped Glyphs)
  3. 光标 (Caret Quad)
- **解耦高亮逻辑**：将语法高亮转化为 `TextRun` 列表，不再参与 UI 树构建。

## 3. 实施步骤

### 阶段一：基础设施构建 (focused_editor.rs)
1. **定义 `FocusedEditorElement`**：实现 `gpui::Element` trait。
2. **状态集成**：使 Element 持有 `DocumentModel` 中关于该块的源码、光标和选区状态。
3. **Layout 逻辑**：
   - 将 `SyntaxHighlighter` 的输出映射为 `Vec<TextRun>`。
   - 调用 `shape_text` 获取并缓存 `WrappedLine`。
   - 根据 `WrappedLine` 汇报准确的 `size` 给 GPUI。

### 阶段二：绘制管线 (Painting)
1. **实现选区绘制**：
   - 利用 `line.x_for_index` 获取像素偏移。
   - 绘制半透明矩形覆盖选区范围。
2. **实现文本绘制**：
   - 直接调用 `line.paint`，利用 GPUI 底层的高性能字形渲染。
3. **实现光标绘制**：
   - 计算光标偏移位置，绘制带有闪烁逻辑（由 Root View 驱动）的绝对定位矩形。

### 阶段三：交互升级 (Hit-testing)
1. **重写点击定位**：
   - 彻底废弃 `clickable_chars`。
   - 在 Element 的 `on_mouse_event` 中使用 `line.closest_index_for_position` 计算偏移。
2. **实现拖拽选区**：
   - 基于统一的 `Element` 坐标系，轻松实现鼠标拖拽扩展选区。

### 阶段四：清理与收口 (shell.rs)
1. **移除旧代码**：删除 `render_highlighted_text`、`render_span_fragment` 等片段拼接逻辑。
2. **挂载 Element**：在 `SolaRoot::render_block` 中直接使用 `FocusedEditorElement`。

## 4. 预期收益
- **极速渲染**：UI 节点数减少 90% 以上。
- **像素级精确**：光标和选区不再有 1px 的对齐抖动。
- ** Zed 级体验**：完全对齐 Zed 的编辑器排版表现，为后续的 inline formula 原生排版打下基础。
