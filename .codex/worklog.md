# Sola 实施工作日志

## 2026-04-23

1. 完成仓库初查：确认当前项目仅有最小化 `Cargo.toml` 与 `src/main.rs`，设计文档位于 `design/sola-architecture-design.md`。
2. 确认本轮首个实施边界：优先落地 Phase 1 基础设施与 Phase 2 的最小可运行原型，即：
   - Cargo workspace 化；
   - gpui 应用入口；
   - 文档模型与块级解析；
   - 基础主题系统；
   - Focused / Blurred 双态块渲染原型。
3. 参考 GPUI 官方 README / docs.rs，确认采用 `Application::new()` + 窗口根视图的组织方式，并按 workspace 方式拆分 crate。
4. 产出规划与测试规格：
   - `.codex/01-planning.md`
   - `.omx/plans/prd-sola-gpui-bootstrap.md`
   - `.omx/plans/test-spec-sola-gpui-bootstrap.md`
5. 完成 workspace 与 crate 拆分：
   - 根 `Cargo.toml` 改为 workspace，并引入 `[workspace.dependencies]`
   - 新增 `crates/sola-app`
   - 新增 `crates/sola-core`
   - 新增 `crates/sola-document`
   - 新增 `crates/sola-theme`
6. 完成首轮代码实现：
   - `sola-document`：基础 Markdown 块模型、focused block 状态、outline 构建、单元测试
   - `sola-theme`：结构化主题模型、TOML 解析、颜色解析、单元测试
   - `sola-app`：GPUI 窗口启动、shell 布局、侧栏、双态块渲染原型
   - 根 `src/main.rs`：仅作为 app 启动入口
7. 架构复审结论已吸收：
   - 保持 `sola-document` UI 无关
   - 保持 `sola-core` 极瘦
   - 本轮暂不做 HTML adapter
8. 验证完成：
   - `cargo fmt --all`
   - `cargo check`
   - `cargo test --workspace`
9. 根据用户更正，将工作记忆目录从 `./codex` 迁移为 `./.codex`，并同步修正文档引用。
10. 修复 `cargo run` 在当前 Linux 容器环境下的 GPUI 启动 panic：
   - 原因：Wayland / X11 后端均不可达，GPUI 启动时 panic
   - 处理：在 `crates/sola-app/src/shell.rs` 增加 Linux 显示后端可达性探测
   - 结果：当前环境下改为输出提示并干净退出，不再 panic
11. 继续推进下一步原型增强：
   - `sola-theme` 新增 `sola_light` 主题
   - `sola-document` 新增 `block_count`、`focused_block_ref`、`focus_next`、`focus_previous`
   - `sola-app` 新增主题切换按钮与 focused block 前后导航按钮
   - 在原型头部显示当前 theme 与 focused block 摘要
12. 本轮修改后再次完成强校验：
   - `cargo fmt --all`
   - `cargo check`
   - `cargo test --workspace`
   - `timeout 10s cargo run`
13. 推进 focused block 的“可编辑源码态雏形”：
   - `sola-document` 为 block 增加 `draft`
   - 支持 `set_focused_draft` / `append_to_focused_draft` / `revert_focused_draft` / `apply_focused_draft`
   - apply 后自动重建 `source` / `outline` / `stats`
   - `sola-app` 为 focused block 增加 `append draft note` / `revert draft` / `apply draft` 控件
14. 本轮修改后再次完成强校验：
   - `cargo fmt --all`
   - `cargo check`
   - `cargo test --workspace`
   - `timeout 10s cargo run`
15. 推进文档结构编辑原型：
   - `sola-document` 新增 `insert_paragraph_after_focused`
   - `sola-document` 新增 `duplicate_focused_block`
   - `sola-document` 新增 `delete_focused_block`
   - apply / insert / duplicate / delete 后统一重建 block id、source、outline、stats
   - `sola-app` 新增 `insert paragraph` / `duplicate block` / `delete block` 按钮
16. 本轮修改后再次完成强校验：
   - `cargo fmt --all`
   - `cargo check`
   - `cargo test --workspace`
   - `timeout 10s cargo run`
17. 继续推进 focused block 的键盘驱动编辑：
   - 查阅 GPUI 本地源码，确认 `KeyDownEvent` / `track_focus` / `FocusHandle` 的接入方式
   - `sola-document` 新增 `push_char_to_focused_draft` / `delete_last_char_from_focused_draft`
   - `sola-app` 为 focused block 接入 `track_focus` 与 `on_key_down`
   - 键盘映射支持：
     - 普通字符 -> 追加到 focused draft
     - Backspace -> 删除最后字符
     - Enter -> 插入换行
     - Escape -> revert draft
     - Ctrl/Cmd+S -> apply draft
18. 本轮修改后再次完成强校验：
   - `cargo fmt --all`
   - `cargo check`
   - `cargo test --workspace`
   - `timeout 10s cargo run`
19. 继续推进快捷键命令面与操作提示：
   - `sola-app` 支持 `Ctrl/Cmd+T` 切换主题
   - `sola-app` 支持 `Alt+↑/↓` 切换 focused block
   - `sola-app` 支持 `Ctrl/Cmd+N` 插入段落、`Ctrl/Cmd+D` 复制 block、`Ctrl/Cmd+Backspace` 删除 block
   - `sola-app` 增加快捷键提示条
20. 本轮修改后再次完成强校验：
    - `cargo fmt --all`
    - `cargo check`
    - `cargo test --workspace`
    - `timeout 10s cargo run`
21. 双态引擎细节打磨：
    - 修复 `on_key_down` 挂载位置问题：原来对所有 block 挂载键盘处理，现在只对 focused block 挂载
    - 新增 draft 视觉反馈：block 头部标签现在显示 "draft"（有未提交的修改）/ "focused"（无修改）/ "blurred"（非当前块）
    - 优化 revert/apply 按钮的激活状态：使用局部 `has_draft` 变量，减少重复调用
    - 退格失败反馈暂不实现（需要光标/选区 UI，超出原型范围）
22. 本轮修改后再次完成强校验：
    - `cargo fmt --all`
    - `cargo check`
    - `cargo test --workspace`
    - `timeout 10s cargo run`
23. 修复鼠标键盘事件无响应问题：
    - 原因：focused block 缺少 `track_focus` 调用，导致 GPUI 焦点系统未正确建立
    - 修复：在 focused block 的 `if is_focused` 分支中添加 `card.track_focus(&self.focus_handle)`
24. 本轮修改后再次完成强校验：
    - `cargo fmt --all`
    - `cargo check`
    - `cargo test --workspace`
    - `timeout 10s cargo run`
25. 再次修复鼠标键盘事件无响应：
    - 原因：只有 blurred block 调用 `track_focus`，focused block 没有，导致焦点路由断裂
    - 修复：将 `track_focus` 移到所有 block 共用的 card 初始化处，确保所有 block 都能参与焦点路由
26. 本轮修改后再次完成强校验：
    - `cargo fmt --all`
    - `cargo check`
    - `cargo test --workspace`
    - `timeout 10s cargo run`
27. 再次修复鼠标键盘事件无响应：
    - 原因：编辑过程中多次修复导致 `on_key_down` 被错误地挂载到所有 block，else 分支也有多余的 `track_focus`
    - 修复：从 git 恢复后重新正确编辑，确保：
      - `track_focus` 只在 card 初始化时调用一次（所有 block 共用）
      - `on_key_down` 只在 `if is_focused` 分支调用
      - 删除 else 分支多余的 `track_focus`
28. 本轮修改后再次完成强校验：
    - `cargo fmt --all`
    - `cargo check`
    - `cargo test --workspace`
    - `timeout 10s cargo run`
29. 继续推进编辑器骨架能力：
    - `sola-document` 新增快照式 `undo/redo` 历史栈
    - 将 focused draft 编辑与结构编辑统一纳入撤销/重做链路
    - 新增 `can_undo` / `can_redo` / `undo` / `redo`
    - 为 draft 编辑、结构编辑、redo 清空行为补充回归测试
30. 为 `sola-app` 接入撤销/重做命令面：
    - 新增 `undo` / `redo` 按钮，并根据历史状态启用/禁用
    - 新增 `Ctrl/Cmd+Z` 撤销
    - 新增 `Ctrl/Cmd+Shift+Z` 与 `Ctrl/Cmd+Y` 重做
    - 快捷键提示条同步更新
31. 本轮修改后再次完成强校验：
    - `cargo fmt --all`
    - `cargo check`
    - `cargo test --workspace`
    - `timeout 10s cargo run`
32. 修复当前原型“鼠标键盘事件都不响应、也无法滚动”的交互失效问题：
    - 根因：GPUI 需要通过 `id()` 将元素提升为 stateful element，按钮 / block card / scroll container 之前是裸 `Div`
    - 结果：虽然低层 `interactivity()` 调用能编译，但点击、焦点键盘、滚动相关状态没有挂到正确的 stateful 交互面
    - 修复：
      - 为所有可点击按钮添加稳定 `id`
      - 为 block card 添加 `id(("block-card", index))`
      - 为文档内容区添加 `id("document-scroll")` 并启用 `overflow_y_scroll()`
      - 为主内容区补 `flex_1` / `min_w_0` / `min_h_0`，确保滚动容器在 flex 布局中生效
33. 本轮修改后再次完成强校验：
    - `cargo fmt --all`
    - `cargo check`
    - `cargo test --workspace`
    - `timeout 10s cargo run`
34. 继续推进 Phase 2 的 HTML adapter 原型：
    - `sola-document` 新增白名单式 HTML adapter 数据模型：`HtmlAdapter` / `HtmlNode` / `HtmlStyledText` / `HtmlImage`
    - 支持提取 `<span style=\"color / font-size\">` 的安全样式
    - 支持提取 `<img>` 的 `src` / `alt` / `width`
    - 遇到不在白名单内或嵌套复杂的 HTML 时标记为 `Unsupported`，供 UI 降级显示
    - paragraph / list / quote 的 `rendered` 摘要改为优先使用 adapter 提炼后的文本
35. 为原型 UI 接入 HTML adapter 预览：
    - 样例文档新增 inline span 与 image adapter 示例
    - `sola-app` 对 adapted HTML 渲染原生预览节点
    - styled span 按颜色与字号做视觉映射
    - image 以原生占位卡片展示 alt / src / width 元数据
    - unsupported HTML 以降级提示 + 源码预览展示
36. 本轮修改后再次完成强校验：
    - `cargo fmt --all`
    - `cargo check`
    - `cargo test --workspace`
    - `timeout 10s cargo run`
37. 推进 Phase 3 的 Tree-sitter 语法高亮集成：
    - 引入 `tree-sitter` 与 `tree-sitter-rust` 依赖。
    - `sola-theme` 扩展 `SyntaxTheme` 模型，支持从 TOML 加载语法配色。
    - `sola-document` 新增 `SyntaxHighlighter`，封装 `tree-sitter::Parser` 并使用 `RefCell` 适配 UI 借用。
    - `sola-app` 接入 `render_highlighted_text` 逻辑，在 focused block 与 CodeFence 中呈现高亮源码。
    - 修复了 `tree-sitter` v0.25 下 `StreamingIterator` 的适配问题。
    - 修复了 Rust 查询中关键字列表的兼容性问题。
38. 本轮修改后再次完成强校验：
    - `cargo fmt --all`
    - `cargo check`
    - `cargo test --workspace`
    - `timeout 10s cargo run`
39. 完成代码提交与推送：
    - Commit Message 详述了 HTML adapter 与 Tree-sitter 的实现决策。
    - 推送至远程仓库 `main` 分支。
40. 推进 Focused Block 内部的光标定位与选区逻辑：
    - `sola-document` 引入 `CursorState` 模型，并实现基于光标的字符插入、文本块删除、左右移动与全选逻辑。
    - `sola-theme` 补充 `selection` 与 `cursor` 语义色。
    - `sola-app` 改造 `render_highlighted_text` 渲染管线，支持在语法高亮片段中切分并插入闪烁光标（稳定版）与选区背景。
    - `sola-app` 接入 `Left` / `Right` / `Shift+Left/Right` / `Ctrl+A` 快捷键，并同步更新快捷键提示条。
41. 本轮修改后再次完成强校验：
    - `cargo fmt --all`
    - `cargo check`
    - `cargo test --workspace`
    - `timeout 10s cargo run`
42. 拆除“脚手架”并实现界面美化与自动保存：
    - `sola-app` 重写块渲染逻辑：移除卡片边框、背景色、状态标签和操作按钮，实现纯净无缝的文档视图。
    - 引入极简选中态提示：在 Focused Block 左侧增加一条 Accent Color 竖线。
    - 实现 `Auto-apply on blur`：在鼠标点击切换块、`Alt+Up/Down` 快捷键导航时，自动触发当前块的草稿保存 (`apply_focused_draft`)。
    - 统一编辑体验：原地编辑源码，失去焦点即渲染为所见即所得态。
43. 本轮修改后再次完成强校验：
    - `cargo fmt --all`
    - `cargo check`
    - `cargo test --workspace`
    - `timeout 10s cargo run`
44. 启动 Phase 4：Typst 数学公式集成：
    - 完成架构规格设计，确立 `sola-typst` crate 的创建和基于 `typst::World` 的在内存中的 SVG 编译流程。
    - 落地实现 `sola-typst`，配置 Typst 依赖、加载 `typst_assets` 字体，完成底层的 `compile_to_svg` 逻辑。
    - 将新模块纳入 cargo workspace 并完成测试。
    - 计划记录并保存至 `.codex/plans/typst-todo.md`。
45. 验证：
    - `cargo check --workspace`
    - `timeout 10s cargo run`
46. 完成 Phase 4 的 `sola-document` / `sola-app` 集成：
    - `sola-document` 新增 `MathBlock` / `TypstBlock` 与 `TypstAdapter::{Pending, Rendered, Error}`。
    - `sola-document` 解析器现可识别 `$$...$$`、多行 `$$` 数学块以及 ````typst```` 代码块。
    - `sola-document` 为 Math/Typst block 维护初始 `Pending` 状态，并在 `rebuild_metadata` 后重新补齐。
    - 为 `sola-document` 增补了针对 parser、状态初始化和 `rebuild_metadata` 的 TDD 回归测试。
47. 在 `sola-app` 接通后台 Typst 渲染与前台预览：
    - `sola-app` 新增 `sola-typst` 依赖，并实现 `typst_render_request` / `apply_typst_result` 两个可测试 helper。
    - `SolaRoot` 新增后台渲染队列，针对 `TypstAdapter::Pending` 块触发 `compile_to_svg`。
    - 渲染完成后回写 `Rendered` / `Error` 状态；对于结构编辑导致的陈旧结果，按 block index + source 做丢弃保护。
    - 由于 GPUI `svg()` 元素依赖 asset path，改用 `img(Image::from_bytes(ImageFormat::Svg, ...))` 展示内存 SVG。
    - 样例文档新增数学公式与 Typst block，便于后续手工验证。
48. 本轮修改后完成强校验：
    - `cargo fmt --all`
    - `cargo test -p sola-document`
    - `cargo test -p sola-app`
    - `cargo test --workspace`
    - `cargo run`（本环境下可编译并进入运行态）
49. 完成“行内数学公式”阶段：
    - `sola-document` 为 paragraph / list / quote 中的 `$...$` 增加检测逻辑，并复用现有 `TypstAdapter` 状态机。
    - `sola-app` 扩展 `typst_render_request`，使 paragraph-like block 也可走 `RenderKind::Block`。
    - `sola-app` 在 blurred 态下对带行内数学的段落型 block 直接展示整块 Typst SVG 预览。
    - 样例文档新增 inline math 例子，便于运行态观察。
50. 行内数学阶段验证：
    - `cargo test -p sola-document`
    - `cargo test -p sola-app`
    - `cargo test --workspace`
    - `timeout 10s cargo run`（完成编译并进入运行态，超时退出符合预期）
51. 完成“鼠标点击定位光标”阶段：
    - `sola-document` 新增 `set_focused_cursor(offset, shift)`，用于统一处理点击后的光标更新、边界夹紧和 `Shift` 选区锚点。
    - `sola-app` focused renderer 现在将可编辑文本拆分为按 UTF-8 边界对齐的可点击字符单元。
    - 点击字符会将光标移动到对应边界，`Shift+Click` 会扩展选区；点击编辑背景会将光标移到当前 block 末尾。
    - block container 点击逻辑增加“仅在切换 block 时才 auto-apply”的保护，避免点击当前 focused block 时误触发保存。
52. 鼠标点击阶段验证：
    - `cargo test -p sola-document`
    - `cargo test -p sola-app`
    - `cargo test --workspace`
    - `timeout 10s cargo run`（完成编译并进入运行态，超时退出符合预期）
53. 完成 Typst 状态保留优化：
    - `sola-document::rebuild_metadata` 现在会在 block 仍然是 Typst/inline-math 且源码未变时保留已有 `TypstAdapter::{Rendered, Error}`。
    - `apply_focused_draft` 在源码变更路径上会显式重建目标 block 的 `TypstAdapter`，确保真正改过的内容仍然重新编译。
    - 为“保留 Rendered 状态”和“保留 Error 状态”分别增加了 TDD 回归测试。
54. Typst 状态保留阶段验证：
    - `cargo fmt --all`
    - `cargo test -p sola-document`
    - `cargo test --workspace`
    - `timeout 10s cargo run`（完成编译并进入运行态，超时退出符合预期）
55. 完成 Typst duplicate 链路优化：
    - `duplicate_focused_block` 现在会复制原 block 的 `typst` 状态。
    - 对已渲染和已报错两种状态分别补充了 TDD 回归测试。
    - 这样复制公式 / Typst / inline-math block 时可以直接复用已有结果，避免立刻进入新的后台编译。
56. Typst duplicate 链路阶段验证：
    - `cargo fmt --all`
    - `cargo test -p sola-document`
    - `cargo test --workspace`
    - `timeout 10s cargo run`（完成编译并进入运行态，超时退出符合预期）
57. 完成 Typst 编译结果缓存：
    - `sola-app` 新增 `typst_cache`，键为 `RenderKind + rendered source`。
    - `trigger_typst_renders` 现在会先查缓存；命中时直接回填 `TypstAdapter`，未命中才进入后台编译。
    - 后台编译完成后会把 `Rendered/Error` 结果写入缓存，供后续相同内容复用。
    - 为缓存 key 和结果映射 helper 增加了 TDD 回归测试。
58. Typst 缓存阶段验证：
    - `cargo fmt --all`
    - `cargo test -p sola-app`
    - `cargo test --workspace`
    - `timeout 10s cargo run`（完成编译并进入运行态，超时退出符合预期）
59. 完成 Typst in-flight 去重：
    - `sola-app` 将 `typst_in_flight` 从按 block index 追踪改为按 cache key 追踪。
    - 同 key 的 pending block 现在在同一轮只会启动一次后台编译。
    - 编译完成后，结果会批量应用到所有仍然匹配该 key 的 pending block。
    - 为“是否应启动编译”和“批量回填缓存结果”两个 helper 增加了 TDD 回归测试。
60. Typst in-flight 去重阶段验证：
    - `cargo fmt --all`
    - `cargo test -p sola-app`
    - `cargo test --workspace`
    - `timeout 10s cargo run`（完成编译并进入运行态，超时退出符合预期）
61. 修复 Typst 共享结果回填路径：
    - 将“后台编译完成后的回填”抽成 `apply_completed_typst_work` helper。
    - 去掉了原先对“发起块 source 未变化”的整批短路依赖，避免 origin block 变化时把同 key 其他 pending block 一起误丢。
    - 为“origin block 已变化，但匹配 peer 仍应收到结果”增加了 TDD 回归测试。
62. Typst 共享结果回填阶段验证：
    - `cargo fmt --all`
    - `cargo test -p sola-app`
    - `cargo test --workspace`
    - `timeout 10s cargo run`（完成编译并进入运行态，超时退出符合预期）
63. 完成显式换行场景下的垂直光标移动：
    - `sola-document` 新增 `move_cursor_up` / `move_cursor_down`，按字符列在显式换行文本间垂直移动，并在短行末尾夹紧。
    - `Shift+↑/↓` 现在可在显式换行场景下扩展选区。
    - `sola-app` 将 `Up/Down` 键接入 focused block 编辑路径，并同步更新快捷键提示。
    - 为垂直移动、短行夹紧、`Shift+↑/↓` 选区补充了 TDD 回归测试。
64. 垂直光标移动阶段验证：
    - `cargo fmt --all`
    - `cargo test -p sola-document`
    - `cargo test --workspace`
    - `timeout 10s cargo run`（完成编译并进入运行态，超时退出符合预期）
