# Sola 应用内级联菜单栏计划 (Phase 5 - 交互重构)

## 1. 目标
目前的 Sola 仅依赖操作系统级别的全局菜单（Global Menu）和零散的 Header 按钮，这在某些 Linux 环境下体验断层。为了实现“平替 Typora”的目标，我们需要在应用窗口的顶层构建一套原生的、跨平台一致的**级联菜单栏（Cascade Menu Bar）**，包含 File、Edit、View 等标准入口。

## 2. 核心架构设计

### A. 状态管理 (`SolaRoot`)
在主视图状态中新增对菜单焦点的追踪：
- `active_menu: Option<&'static str>`：记录当前处于展开状态的顶级菜单项（如 `"File"`, `"Edit"`）。
- 当一个菜单展开时，鼠标悬停在其他顶级菜单上会自动切换 `active_menu`，这也是传统桌面应用的标准体验。

### B. 菜单数据结构
为了便于维护和渲染，定义简单的声明式菜单结构：
```rust
struct MenuDefinition {
    label: &'static str,
    items: Vec<MenuItemDef>,
}

enum MenuItemDef {
    Action { label: &'static str, shortcut: Option<&'static str>, action: Box<dyn Fn(&mut SolaRoot, &mut Context<SolaRoot>)> },
    Separator,
}
```
*注：由于要复用现有的 Action，这里的 action 闭包将直接调用现成的业务逻辑（如 `open_project`, `undo` 等）。*

### C. UI 渲染管道
1. **Menu Bar (横栏)**：在 `render_header` 区域上方或内部渲染一条高对比度/暗色的横条。放置 `File`, `Edit`, `View` 等按钮。
2. **Menu Overlay (下拉层)**：
   - 如果 `active_menu` 存在，使用 `.absolute()` 在对应的顶级按钮正下方渲染一个具有 `z_index`、阴影和边框的列表面板。
   - 渲染对应的 `MenuItemDef`，支持 Hover 变色和点击执行。
3. **Click Outside (全局关闭)**：
   - 渲染下拉菜单的同时，在菜单的底层（Z-index 低于菜单但高于应用主内容）渲染一个全屏的、透明的 `div`。
   - 监听这个遮罩的 `on_mouse_down`，点击即执行 `this.active_menu = None`，实现“点击空白处关闭菜单”的体验。

## 3. 实施步骤
1. **状态扩充**：在 `SolaRoot` 中添加 `active_menu` 状态。
2. **编写菜单配置**：在 `shell.rs` 中声明标准的三大菜单（File, Edit, View）及其包含的子项。
3. **渲染逻辑**：
   - 实现 `render_menu_bar` 取代之前零散的 `open_btn` 和 `save_btn`。
   - 实现 `render_active_menu_overlay`。
4. **交互联动**：打通悬停自动切换与全局点击关闭机制。
