# Sola 偏好设置面板与主界面瘦身计划 (Phase 5 - UI 纯净版)

## 1. 目标
目前的 Sola 主界面依然残留着大量原型阶段的“调试控制台”气息（如 `STRUCTURE EDITOR` 按钮组、`DOCUMENT SURFACE` 占位、`KEYBOARD SHORTCUTS` 列表等）。这与 Typora / Zed 追求的“沉浸式极简”理念背道而驰。
本阶段将：
1. **彻底瘦身主界面**：移除所有与文本编辑无关的按钮和统计信息，只保留侧边栏、顶部 Tab 栏和主编辑区。
2. **实现偏好设置 (Preferences) 面板**：将快捷键列表、主题切换等系统级配置项移入一个原生的模态对话框中，并通过 `File -> Preferences` (或 `Ctrl+,`) 唤起。

## 2. 核心架构设计

### A. 状态扩展 (`SolaRoot`)
在 `SolaRoot` 中新增一个控制偏好设置面板开关的状态：
```rust
pub struct SolaRoot {
    // ... existing fields ...
    show_preferences: bool,
}
```
初始状态为 `false`。

### B. 偏好设置模态框 (Preferences Modal)
实现一个新的渲染方法 `render_preferences_modal(&self, cx: &mut Context<Self>) -> Option<Div>`。
- **遮罩层 (Mask)**：点击半透明背景可关闭该面板。
- **弹窗主体 (Modal Container)**：固定宽度（如 600px）居中显示。
- **内容划分**：
  - **General (常规)**：放置 `Toggle Theme` 按钮，显示当前的 `Sola Dark` / `Sola Light` 状态。
  - **Keyboard Shortcuts (快捷键)**：将原本渲染在编辑器底部的 `shortcut_legend` 完整迁移至此。

### C. 视图层大瘦身 (UI Cleanup)
重构 `shell.rs` 中的渲染链路：
1. **移除 `render_header`**：顶部的 "Sola" 名称、"workspace" 药丸状态、"roadmap" 全部移除。
2. **重构 `render_document_surface`**：
   - 彻底删除所有的 Action Button 渲染（`previous_button`, `next_button`, `insert_button`, `duplicate_button`, `delete_button`, `undo_button`, `redo_button`）。这些功能现在完全由菜单栏、右键菜单和快捷键接管！
   - 删除所有 `section_title` 占位符。
   - `render_document_surface` 只需要返回一个铺满全屏 (`size_full()`) 的 `gpui::list`（即虚拟化文档块序列）。

### D. Action 联动
- 在 `actions.rs` 中确保 `Preferences` 动作存在。
- 在 `handle_focused_key_down` 中拦截 `Ctrl+,` 快捷键，触发 `this.show_preferences = true; cx.notify();`。
- 在 `render_overlay_item` 中对接 `Preferences` 菜单项逻辑。

## 3. 实施步骤
1. **状态与 Action 绑定**：在 `SolaRoot` 新增状态并打通触发逻辑。
2. **编写 Preferences 弹窗**：实现带有磨砂质感遮罩的设置中心。
3. **“屠宰式”清理主界面**：大刀阔斧地删除 `render_document_surface` 中数百行冗余 UI 代码。
4. **编译验证**：启动 Sola，确认界面变得如同白纸般纯净，按 `Ctrl+,` 能够流畅唤出设置面板。
