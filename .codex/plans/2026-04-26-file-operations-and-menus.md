# Sola 文件操作与上下文菜单计划 (Phase 5 - 扩展)

## 1. 目标
补齐 Sola 作为本地编辑器的最后一环：实现侧边栏文件树的右键上下文菜单（Context Menu），支持在应用内直接新建文件/文件夹、重命名以及删除操作。

## 2. 核心设计

### A. 操作体系 (Actions & IO)
在 `actions.rs` 中扩展全局指令：
- `NewFile`
- `NewFolder`
- `Rename`
- `Delete`

在 `workspace.rs` 中增加对应的 IO 闭环方法：
- `create_file(parent_dir, name)`
- `create_dir(parent_dir, name)`
- `rename_entry(old_path, new_path)`
- `delete_entry(path)`
这些方法执行完原生 `std::fs` 操作后，由于我们有 `notify` 后台监听，`Worktree` 会自动感知并刷新 UI，无需手动重建树！

### B. 上下文菜单 UI (Context Menu)
在 `ProjectPanel` 的状态中新增 `active_context_menu: Option<(PathBuf, gpui::Point<Pixels>)>`。
- **触发**：在虚拟列表的每一项上监听 `on_mouse_down` (按键为 `Right`)，更新右键菜单的激活路径与坐标。
- **渲染**：在 `ProjectPanel` 或 `WorkspaceView` 的最外层追加一个绝对定位 (`absolute`)、高 z-index 的 `div`，渲染菜单项。
- **交互**：点击菜单外的区域时自动关闭菜单。

### C. 简单的重命名/新建交互 (Prompt)
由于 GPUI 0.2 原生输入框组件较为复杂，我们将采用以下极简方案实现重命名/新建：
- 当触发 `NewFile` 或 `Rename` 时，调用 `cx.prompt`（如果 GPUI 提供了系统输入对话框），或者：
- 渲染一个临时的“输入状态行”在选中的文件树节点下方，监听键盘输入，按下 `Enter` 后执行 IO，按下 `Esc` 取消。

## 3. 实施步骤
1. **完善 IO 层**：在 `Workspace` 添加文件系统的增删改方法，并处理常见的系统错误。
2. **实现右键菜单 UI**：在 `ProjectPanel` 增加右键菜单的弹窗渲染逻辑与状态管理。
3. **绑定 Action**：将右键菜单的点击事件路由至第一步实现的 IO 方法。
4. **验证联动**：测试新建文件后，后台 `notify` 是否能正确触发文件树的自动刷新。
