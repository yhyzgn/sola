# Sola Typst Integration Design

## 1. 概览 (Overview)
Sola 作为一个专注技术排版的编辑器，将原生集成 Typst 引擎以支持高质量的数学公式和高级布局渲染。此方案采用**原生库直连 (Native Crate Integration)**，并支持基础公式与高级混编。

## 2. 目标与范围 (Goals & Scope)
**包含:**
- 支持 `$x$` (行内数学公式)。
- 支持 `$$x$$` (块级数学公式)。
- 支持 ````typst ... ```` (Typst 代码块混编)。
- 创建 `sola-typst` crate，封装 Typst 编译器，实现在内存中无文件落地的 SVG 生成。
- 在 `sola-document` 模型中扩展 `TypstAdapter` 和对应的 AST 节点。
- 在 `sola-app` 中集成后台异步编译逻辑与前台 SVG 渲染 (`gpui::svg`)。

**不包含:**
- 多文件引用 (`#import`) 等需要完整物理文件系统支持的 Typst 特性。
- 通过网络下载字体 (目前仅打包最基本的数学字体库，如 Libertinus Math)。

## 3. 架构设计 (Architecture)

### 3.1 模块划分
新增一个 workspace 成员 `crates/sola-typst`。这是为了：
- 隔离 Typst 庞大的依赖包，防止 Sola 主循环被拖慢。
- 封装 `typst::World` 的复杂实现，对上层暴露出极简的接口 `compile_to_svg(source, kind)`。

### 3.2 领域模型扩展 (`sola-document`)
在 `HtmlAdapter` 之外，引入独立的适配器 `TypstAdapter`：
```rust
pub enum TypstAdapter {
    Pending,                      // 尚未编译完成
    Rendered { svg: String },     // 编译成功，持有 SVG 数据
    Error { message: String },    // 语法错误
}
```
`BlockKind` 增加变体：
- `MathBlock` (对应 `$$`)
- `InlineMath` (通过 Markdown 解析器识别段落中的 `$`)

### 3.3 Typst World 实现 (`sola-typst`)
- **Virtual File System (VFS)**：实现一个在内存中管理的 VFS，供 Typst `World` trait 读取。
- **Font Provider**：加载内置的字体字节流 (如 Libertinus Math) 以支持符号渲染。
- **Templates**：对纯数学公式包裹隐藏模板：
  ```typst
  #set page(width: auto, height: auto, margin: 0pt)
  #set text(fill: {theme_color})
  $ {user_math} $
  ```

### 3.4 异步渲染流 (`sola-app`)
由于 Typst 编译（尤其冷启动时）耗时可能超过 16ms，为了保证输入流畅：
- 当块失焦变为 `Blurred` 且包含 Typst/Math 节点时，状态置为 `Pending`。
- `sola-app` 触发 `cx.background_executor().spawn(...)`。
- 编译完成后，将生成的 `svg` 发送回主线程，更新对应 `DocumentBlock`，并调用 `cx.notify()`。
- GPUI 根据 `TypstAdapter` 状态进行渲染：`Pending` 显示 Loading，`Rendered` 显示图像，`Error` 显示带红色的源码和错误信息。

## 4. 自我审查 (Spec Self-Review)
1. **Placeholder scan**: 没有 TDD、TODO，接口定义清晰。
2. **Internal consistency**: 异步流和模型更新不冲突。
3. **Scope check**: 适中。建立底层 Crate、扩展 AST 并连接渲染管线，可以在一个功能周期内完成。
4. **Ambiguity check**: 明确指出了初期不支持多文件 `import`。

## 5. 迁移与风险 (Risks)
- **编译体积与速度**: 引入 Typst 会导致首次 `cargo build` 时间显著增加，这是原生集成的必然代价，可被接受。
- **字体打包**: 需在 `sola-typst` 中打包必要的中英文字体和数学字体（可能会增加几MB的二进制体积）。