# Sola 原生行内公式排版计划 (Phase 5 - 压轴)

## 1. 目标
作为一款对标 Typora 的 Markdown 编辑器，Sola 必须提供“所见即所得”的行内公式（Inline Math, `$ e = mc^2 $`）渲染能力。我们不仅要在段落块中渲染文字，还要在文字之间无缝嵌入由 Typst 渲染出的高清 SVG 公式。

## 2. 核心架构设计

### A. 解析层支持 (`sola-document`)
在 Markdown AST 遍历时，识别同行文本内的行内公式语法（通常由 `$ ... $` 包裹）。
- 在 `HtmlNode` 或 `Span` 类型中增加 `InlineMath(String)`。
- 为这些行内公式生成独立的 `TypstAdapter::Pending` 状态，并为其分配专门的 Cache Key（带有 `inline_math_` 前缀以区分块级公式）。

### B. 渲染引擎增强 (`sola-app` / `shell.rs`)
当 `Workspace` 触发 Typst 渲染队列时，将包含行内公式的 `DocumentBlock` 也纳入渲染范畴。
- `typst_render_request` 需要遍历段落内部的 Spans，找出所有 `InlineMath` 节点。
- 后台调用 `compile_to_svg`，生成高度适配当前字号（例如与 `1em` 对应的 px 尺寸对齐）的无边距公式 SVG。

### C. 视图排版层接入 (`focused_editor.rs` & GPUI)
GPUI 原生支持将元素插入文本流。
- 在 `spans_to_runs` 函数中，遇到 `InlineMath` 并且其 `TypstAdapter` 已就绪（包含 SVG 字符串）时，我们不仅生成文本 `Run`，还需在此处插入一个特殊的 `gpui::Image` 占位或自定义绘制逻辑。
- 考虑到 GPUI 的底层结构，我们可以利用 `gpui::AnyElement` 包装 `img().source(svg_data)`，并将其作为内联装饰（Decoration）或是将其切分为多个文本段，在中间留出空白，然后在对应的坐标上绝对定位绘制 SVG。
- **最佳实践**：如果 GPUI 0.2 `Text` 组件支持自定义的 `custom_run` 或行内占位符，我们将使用它来保证换行与基线的正确对齐。

## 3. 实施步骤
1. **完善模型**：修改 `DocumentModel` 增加对行内公式的解析。
2. **连接队列**：修改 `trigger_typst_renders` 使其能提取并缓存行内公式的 SVG。
3. **探索排版 API**：研究 GPUI 0.2 中实现“文字混排 SVG”的最佳方式，若无官方内联图片 API，则采用“计算文字宽度后绝对定位叠加”的策略。
4. **编译与验证**：输入 `$ \sum_{i=1}^n i^3 = (\frac{n(n+1)}{2})^2 $`，验证其在段落中正确渲染并与上下文基线对齐。
