# Sola 首轮实现规划

## 总体目标

基于 `design/sola-architecture-design.md`，先把项目从空白 Rust 工程推进到一个可演进的 **GPUI + Cargo workspace** 骨架，并落地“Markdown 块级双态渲染”的第一版原型。

## 本轮实施范围

### 纳入

1. 将仓库改造成 workspace，并抽取必要 crate 到项目根目录下的顶层 `crates/` 目录。
2. 引入 GPUI 作为 GUI 主体技术。
3. 建立文档域模型、Markdown 解析、主题模型、App 壳层。
4. 实现最小可运行的编辑器界面：
   - 左侧文档结构/阶段信息；
   - 顶部工具条；
   - 中间块级内容区；
   - 点击块时切换 Focused/Blurred 状态。
5. 为后续 Tree-sitter / Typst / 导出流水线预留 crate 边界。

### 暂不纳入

1. 真正的文本编辑器输入系统（Undo/Redo、光标移动、选区、IME）。
2. Tree-sitter 高亮、Typst 公式、Mermaid、导出 PDF/长图。
3. 文件系统监听、拖拽图片、平台特化。

## crate 规划

- `src/main.rs`：根包入口，仅负责启动 app crate。
- `crates/sola-app`：GPUI 应用层、窗口根视图、页面布局。
- `crates/sola-core`：应用常量、启动配置、示例文档等共享核心类型。
- `crates/sola-document`：Markdown 文档模型、块类型、解析与选择状态。
- `crates/sola-theme`：结构化主题模型与默认主题。

## 实施步骤

1. 重构 Cargo workspace 与共享依赖。
2. 建立 `sola-core / sola-document / sola-theme / sola-app` 四个 crate。
3. 在 `sola-document` 中实现：
   - `DocumentModel`
   - `DocumentBlock`
   - Markdown 到块列表的最小解析
   - Focused block 选择状态
4. 在 `sola-theme` 中实现默认主题与语义色板。
5. 在 `sola-app` 中实现 GPUI 根视图：
   - App shell
   - 工具条
   - 侧栏
   - 块列表渲染
   - Focused/Blurred 原型交互
6. 根包 `src/main.rs` 调用 app 启动。
7. 运行 `cargo fmt`、`cargo check`、`cargo test` 验证。

## 关键设计决策

1. **先做块级状态机，而不是直接做富文本编辑器。**
   - 理由：设计文档明确 Sola 的核心是块级双态引擎。
2. **先拆 domain/theme/app crate，再扩展解析/渲染能力。**
   - 理由：用户要求 workspace 化与根级 crate 拆分。
3. **先用最小 Markdown 块识别覆盖标题/段落/列表/引用/代码块。**
   - 理由：足够支撑第一版双态原型，又能控制复杂度。
