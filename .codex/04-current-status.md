# Sola Current Status & Handover Memory

## 当前阶段 (Current Phase)
- **Phase 1-4 核心功能已就绪**：建立了 GPUI 骨架，完成了双态引擎原型、HTML 适配器、Tree-sitter 语法高亮、撤销/重做系统，并接入了 Typst 数学公式 / 代码块渲染链路。
- **光标与选区逻辑已落地**：Focused Block 现在支持通过方向键移动光标、Shift 组合键选区以及全选操作。
- **垂直方向键已补齐基础能力**：Focused Block 现在支持 `↑/↓` 和 `Shift+↑/↓` 在显式换行的多行文本间移动光标和扩展选区。
- **鼠标点击定位已接入**：Focused Block 现在支持通过鼠标点击可见字符将光标移动到相应 UTF-8 边界，并支持 `Shift+Click` 扩展选区。
- **同块点击后的键盘焦点已修复**：即使点击当前已经 focused 的 block，编辑器现在也会重新请求窗口焦点，避免随后左右/上下键没有路由进编辑器。
- **键盘事件路由已修正**：Focused Block 的 `track_focus` 和 `on_key_down` 现在挂在同一可聚焦元素上，避免“看起来已聚焦但键盘事件没有真正送达”的情况。
- **编辑面焦点模型已收敛**：编辑区现在使用单一可聚焦 surface 统一承接键盘输入，不再让多个 block 复用同一个 `focus_handle` 争抢焦点。
- **键盘输入根因已回到框架用法层修复**：这次对照 GPUI 官方 `input.rs` 示例后，编辑区的 `track_focus` 与 `on_key_down` 已统一落到同一个 editor surface，上层 block 仅保留点击/展示职责。
- **编辑体验基础观感已收紧**：Focused 编辑区现在具备基础光标闪烁，并收紧了代码编辑区的行高、内边距与字符排版，减少“看着像一堆块而不是文本编辑器”的割裂感。
- **光标与代码渲染抖动已进一步修正**：光标现在改为零宽视觉，不再在闪烁/移动时把后文顶开；代码区走单行保留 + 横向滚动路径，减少本不该出现的假换行。
- **Caret 覆盖绘制已走正路**：当前 focused caret 已改为绝对定位覆盖绘制，不再通过会参与文本流布局的节点来冒充光标。
- **编辑区错行问题已继续压缩**：代码区现优先保留显式行结构，HTML 适配文本去掉了固定节点间距；这降低了“本该同一行却被拆开”的概率，但完整的文本引擎级 inline rich text 排版仍未完成。
- **Focused editor 重构底座已建立**：`sola-app` 现已抽出独立的 `focused_editor` 模块，用于承载后续基于 GPUI 文本布局系统的真实编辑面迁移。
- **软换行级垂直移动已开始接入真实布局**：`focused_editor` 现在已具备基于 `shape_text / WrappedLine` 的视觉行 helper，`↑/↓` 开始改走真实布局推导，而不是只依赖显式换行。
- **Wrapped layout hit-testing groundwork 已建立**：`focused_editor` 现已补齐视觉行 y 命中和 wrapped layout offset 命中 helper，为下一步把点击定位正式迁到真实文本布局模型做好准备。
- **视觉行 Home/End 已接入**：Focused 编辑区现在会优先把 `Home/End` 解释为“当前视觉行的行首/行尾”，而不是整段文本的绝对开头/结尾。
- **点击定位已开始接入 wrapped layout**：Focused 编辑区的背景点击现在优先走 wrapped text layout 命中，而不是直接粗暴落到段尾。
- **Focused editor 已进入 TextLayout/WrappedLine 迁移期**：虽然主渲染仍以现有 highlighter/flex 方案为主，但软换行级别的 `↑/↓`、`Home/End` 和背景点击已经开始共享 wrapped layout helper，后续将继续把字符点击、选区与 caret 绘制迁到同一模型上。
- **界面已完成“拆除脚手架”美化**：移除了冗余的卡片边框、按钮和标签，实现了纯净的文档视图；引入了 `Auto-apply on blur` 交互，实现了原地编辑与自动保存的无缝衔接。
- **Typst 预览已扩展到行内公式**：`sola-document` 现在还能为包含 `$...$` 的段落/列表/引用块建立 `TypstAdapter` 状态，`sola-app` 会将这些 blurred block 作为整块 Typst 文本进行异步预览。
- **离线导出流水线已启动第一阶段**：新增独立的 `sola-export` crate，当前已支持导出当前文档的 `Markdown` 与带主题样式注入的静态 `HTML`。

## 关键架构决策 (Key Decisions)
1. **Tree-sitter 借用优化**：由于 GPUI 渲染闭包需要不可变借用，而 `tree-sitter::Parser` 的 `parse` 需要 `&mut self`。为了避免在 UI 层传递可变引用，`SyntaxHighlighter` 内部使用了 `RefCell<Parser>`，实现了渲染层的“逻辑不可变”访问。
2. **HTML 适配策略**：坚决不引入浏览器引擎。目前的 `HtmlAdapter` 采用白名单提取模式，将 `style="color; font-size"` 和 `img[width]` 映射为原生 GPUI 布局约束。
3. **环境适配**：针对无桌面的 Linux 容器环境，在 `sola-app/src/shell.rs` 中实现了后端探测逻辑，避免启动时 panic。
4. **声明式光标渲染**：通过在 `render_highlighted_text` 遍历 span 时，根据字节偏移量逻辑切分 span 并插入 `Div` 元素，实现了语法高亮、选区背景与光标的融合渲染。
5. **沉浸式双态交互**：放弃了复杂的“编辑/预览”双栏切换，采用“原地替换 + 自动保存”的模式。通过移除所有装饰性组件，将应用从“调试工具”转变为真正的“创作工具”。
6. **动态 SVG 渲染策略**：GPUI 当前的 `svg()` 元素面向 asset path，不适合直接消费内存中的 SVG 字符串。因此 Typst 的渲染结果通过 `img(Image::from_bytes(ImageFormat::Svg, ...))` 进入 UI，避免了落地临时文件。

## 剩余技术债/风险 (Risks & Tech Debt)
- **垂直移动仍未完全覆盖视觉软换行**：`↑/↓` 已开始接入 wrapped layout helper，但当前只是优先尝试该路径，核心渲染面仍未完全迁移到统一的 TextLayout/WrappedLine 模型。
- **Tree-sitter 关键字**：在 Rust 查询中，`mut` 等部分关键字作为字符串字面量查询时在 v0.25 下会报 `NodeType` 错误，目前已在查询中暂时规避。
- **Typst 状态保留已优化**：未变更源码的 Math/Typst/inline-math block 现在会保留已有 `Rendered/Error` 状态，不再在常规 `rebuild_metadata` 后无差别退回 `Pending`。
- **Typst 复制链路已优化**：复制一个已渲染或已报错的公式 / Typst / inline-math block 时，新块现在会直接继承现有 `TypstAdapter`，避免立即触发无意义的重复编译。
- **Typst 结果缓存已接入**：`sola-app` 现在会基于 `RenderKind + rendered source` 缓存 Typst 编译结果。重复出现的相同内容会直接命中缓存，而不是再次启动后台编译。
- **Typst 并发去重已接入**：当同一轮里有多个完全相同的 pending Typst 请求时，`sola-app` 现在只会启动一次后台编译，并在结果返回后批量回填所有匹配 block。
- **Typst 共享结果回填已修正**：即便最初发起编译的那个 block 在结果返回前已经变更内容，同 key 的其他 pending block 仍会正确拿到这次共享编译结果，不会被误丢。
- **行内公式仍是整块预览**：当前 paragraph/list/quote 中的 `$...$` 通过整块 Typst SVG 呈现，而不是在原生文本节点中做逐公式内嵌渲染；这是当前最小可行实现。
- **点击命中仍未完全统一到真实布局**：背景点击已经优先走 wrapped layout 命中，但字符点击仍主要沿用旧的 flex 片段路径，软换行场景下还存在不一致。
- **软换行与视觉行距仍需进一步精修**：当前行高已经收紧，但 `↑/↓` 仍只覆盖显式换行，真正的视觉软换行移动和更精确的文本排版还需继续推进。
- **HTML 适配文本仍需更精细的行内布局**：当前已去掉节点间固定 gap，但真正严丝合缝的 inline rich text 排版还需要从 flex 拼接过渡到更接近文本引擎的布局模型。
- **Focused 编辑区仍未完全切到 TextLayout/WrappedLine 驱动**：目前已经把 wrapped layout helper 接到部分命令面，但真正的文本绘制、selection 计算和字符级 hit-testing 还没整体迁移完成。

## 当前正在做 (Current In-Flight Direction)
- **大方向**：把 Focused 编辑区从 `flex + span fragment` 方案迁到 GPUI 文本布局驱动的 custom text surface。
- **当前子目标**：统一 `↑/↓`、`Home/End`、点击命中到同一套 wrapped layout helper 上。
- **下一步优先级最高的任务**：
  1. 把字符点击从旧 `clickable_chars` 路径迁到 wrapped layout hit-testing。
  2. 把 caret / selection 从 fragment 级视觉拼接迁到真实文本布局下的统一绘制。
  3. 在这两步稳定后，再继续提升软换行场景下的 `↑/↓` 精度与一致性。

## 下一步建议 (Next Steps)
- 继续把 `sola-export` 从 `Markdown/HTML` 扩展到真正的 PDF / 长图目标。
- 支持更细粒度的 Typst 脏块重渲染与真正的原生 inline formula 布局。
- 继续完成 Focused editor 的 TextLayout/WrappedLine 迁移，优先统一 click hit-testing、caret、selection。
