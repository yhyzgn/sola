# Sola 对齐 Typora 的菜单重构计划 (Phase 5 - 最终完善)

## 1. 目标
补齐 Sola 菜单栏的最后几块功能拼图，深度对齐 Typora：
- 引入 **Import (导入)** 和 **Export (导出)** 子菜单。
- 引入 **Preferences (偏好设置)** 入口。
- 独立出顶层 **Themes (主题)** 菜单，包含 Dark / Light / 更多切换选项。
- 扩充 **Edit (编辑)** 和 **View (视图)** 菜单的功能占位。
- **修复 Bug**：修复菜单弹窗后点击外部区域无法正确关闭的体验问题。

## 2. 核心架构设计

### A. 菜单体系扩充 (Menubar & Overlay)
在 `render_menu_overlay` 的模式匹配中扩充：
- **File**: New, Open..., Open Folder..., Open Recent >, [Sep], Save, Save As..., [Sep], Import >, Export >, [Sep], Preferences..., [Sep], Close, Quit.
- **Edit**: Undo, Redo, [Sep], Cut, Copy, Paste, [Sep], Select All, [Sep], Search. (目前绝大多数由系统或未来实现提供，Sola 先打通 UI 占位)。
- **View**: Toggle Sidebar, Focus Mode, Typewriter Mode, [Sep], Source Code Mode.
- **Themes**: 提供 `Sola Dark` 和 `Sola Light`，直接对应当前的 `toggle_theme` 操作。

### B. Bug 修复：全屏遮罩 (Mask) 拦截
目前 `render_menu_mask` 没有如期工作。
- **原因分析**：在 GPUI 0.2 中，如果 Mask 所在的容器没有吸收点击事件，事件可能会穿透，或者由于 Z-Index 问题未置于顶层。
- **解决方案**：在渲染主应用结构的最后（即 `.when_some` 处），渲染一个绝对定位、宽高 100%、高 `z_index` 但背景完全透明的层，显式监听并**拦截 (Stop Propagation)** 鼠标事件，调用 `this.active_menu = None` 和 `this.active_submenu = None`，然后 `cx.notify()`。

### C. 动作扩充 (Actions)
在 `actions.rs` 中预留新增的指令：
- `Import`, `Export`, `Preferences`。
- 虽然第一阶段可能仅是占位，但它们保证了菜单栏在视觉和操作逻辑上能与 Typora 站在同一高度。

## 3. 实施步骤
1. **更新 Actions**：在 `actions.rs` 添加一系列对应菜单的新动作。
2. **重构菜单配置**：重写 `render_menu_overlay` 中的配置字典，使其完全反映 Typora 的功能树。实现 `Import >` 和 `Export >` 的子菜单（如 Export -> PDF / HTML）。
3. **修复遮罩 Bug**：增强 `render_menu_mask`，确保它的层级和事件捕获能力，并阻止事件冒泡。
4. **编译与验证测试**：测试各个顶级菜单的悬停响应，验证多级子菜单展开，点击外部的即时关闭逻辑。
