# Sola Current Status & Handover Memory

## 当前阶段 (Current Phase)
- **Phase 1-3 核心功能已就绪**：建立了 GPUI 骨架，完成了双态引擎原型、HTML 适配器、Tree-sitter 语法高亮、撤销/重做系统。
- **光标与选区逻辑已落地**：Focused Block 现在支持通过方向键移动光标、Shift 组合键选区以及全选操作，渲染层实现了高亮片段中的光标与选区切片。

## 关键架构决策 (Key Decisions)
1. **Tree-sitter 借用优化**：由于 GPUI 渲染闭包需要不可变借用，而 `tree-sitter::Parser` 的 `parse` 需要 `&mut self`。为了避免在 UI 层传递可变引用，`SyntaxHighlighter` 内部使用了 `RefCell<Parser>`，实现了渲染层的“逻辑不可变”访问。
2. **HTML 适配策略**：坚决不引入浏览器引擎。目前的 `HtmlAdapter` 采用白名单提取模式，将 `style="color; font-size"` 和 `img[width]` 映射为原生 GPUI 布局约束。
3. **环境适配**：针对无桌面的 Linux 容器环境，在 `sola-app/src/shell.rs` 中实现了后端探测逻辑，避免启动时 panic。
4. **声明式光标渲染**：没有采用底层的 TextLayout 指令，而是通过在 `render_highlighted_text` 遍历 span 时，根据字节偏移量逻辑切分 span 并插入 `Div` 元素。这种方式在原型期能快速实现语法高亮、选区背景与光标的融合渲染。

## 剩余技术债/风险 (Risks & Tech Debt)
- **垂直移动逻辑**：目前尚未实现上下方向键跨行移动光标，这需要对 `Flex-wrap` 排版下的字符坐标有精确测量。
- **Tree-sitter 关键字**：在 Rust 查询中，`mut` 等部分关键字作为字符串字面量查询时在 v0.25 下会报 `NodeType` 错误，目前已在查询中暂时规避。

## 下一步建议 (Next Steps)
- 引入 `Typst` 实现数学公式渲染。
- 完善鼠标点击定位光标的交互。
- 搭建离线导出流水线。
