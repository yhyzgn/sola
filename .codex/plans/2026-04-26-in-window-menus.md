# 修复 Sola 菜单与快捷键不响应计划

## 1. 问题描述
用户反馈无法在页面上看到“文件”菜单，且按下 `Ctrl+O` 毫无反应。
这主要由两个原因导致：
1. **Linux 桌面环境特性**：`cx.set_menus` 生成的是系统级别的全局菜单（Global Menu），在某些 Linux 桌面管理器（如非 KDE/GNOME 或缺少 DBus 支持的 WM）中可能不会显示在应用窗口内。
2. **GPUI 焦点机制**：GPUI 的 `Action` 快捷键只有在相应的组件（携带 `track_focus` 和 `on_action` 监听器的元素）获得了**焦点 (Focus)** 时才会被触发。应用启动时主视图尚未获焦。

## 2. 修复方案

### A. 强制初始获焦 (Focus Initialization)
在 `shell.rs` 的 `run` 函数中，打开窗口并创建 `SolaRoot` 实体后，显式调用 `window.focus(&handle.read(cx).focus_handle)`。
这样可以确保应用一旦启动，最顶层的视图立刻拦截键盘输入。

### B. 页面内置菜单栏 (In-Window Action Bar)
为了不依赖系统特有的全局菜单栏，同时给用户直观的操作入口：
- 改进 `SolaRoot::render_header` 方法。
- 在页面顶部除了“切换主题”，显式增加 **“Open File/Folder...”** 和 **“Save (Ctrl+S)”** 等 `action_button`。
- 将这些按钮的点击事件通过 `cx.dispatch_action` 直接绑定到对应的 `Open` 和 `Save` 全局动作上，复用核心逻辑。

## 3. 实施步骤
1. 打开 `crates/sola-app/src/shell.rs`。
2. 在 `cx.open_window` 的初始化闭包中，添加窗口焦点赋权逻辑。
3. 重构 `render_header` 方法，新增常驻的顶部操作按钮栏（Toolbar）。
4. 验证 `Ctrl+O` 快捷键的生效，以及页面顶部原生菜单按钮的可用性。
