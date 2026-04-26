# Sola 全功能级联菜单计划 (对齐 Typora)

## 1. 目标
目前的 Sola 仅有基础的 File (Open, Save, Quit) 和 Edit 菜单。为了实现“平替 Typora”的产品承诺，我们需要将其升级为完整的桌面级菜单栏，特别要对齐 Typora 的 **File (文件)** 菜单功能（如：新建、打开文件、打开文件夹、最近使用的文件、保存、另存为等）。

## 2. 核心架构设计

### A. 最近文件持久化 (Recent Files)
- 引入全局配置或利用 GPUI 的 `LocalShared` (如果可用) 存储最近打开的文件/文件夹列表。
- 考虑到跨会话持久化，可以在用户主目录下创建一个极简的配置文件（如 `~/.sola/recent.json`），或先在内存中实现逻辑闭环（配合 `workspace.rs`）。
- 在 `Workspace` 实体初始化时读取这些路径；每次成功打开文件/目录时更新该列表。

### B. 菜单数据结构升级 (Submenus)
现有的 `render_menu_overlay` 是一维列表，无法渲染“Open Recent >”这种级联子菜单。
- 将 `items` 的定义升级为树形结构：
  ```rust
  enum MenuItemDef {
      Action { label: String, shortcut: Option<String>, action: Box<dyn Fn(&mut SolaRoot, &mut Context<SolaRoot>)> },
      Separator,
      Submenu { label: String, items: Vec<MenuItemDef> },
  }
  ```
- 新增 `active_submenu: Option<&'static str>` 状态来追踪当前展开的子菜单（悬浮展开）。

### C. 动作扩充 (Actions & Dialogs)
在 `actions.rs` 中新增以下全局操作：
- `NewWindow` (Ctrl+N): 打开一个新的 Sola 实例或清空当前工作区。
- `OpenFile` (Ctrl+O): `prompt_for_paths` 设置为 `directories: false`。
- `OpenFolder` (无快捷键或 Ctrl+Shift+O): `prompt_for_paths` 设置为 `files: false`。
- `SaveAs` (Ctrl+Shift+S): 使用 `cx.prompt_for_new_path` 获取保存路径，然后执行写盘操作并更新 `Workspace` 的 `current_path`。

### D. File 菜单全景图 (对齐 Typora)
渲染以下结构：
- **New** (Ctrl+N)
- **New Window** (Ctrl+Shift+N) - 可选
- [Separator]
- **Open File...** (Ctrl+O)
- **Open Folder...** (Ctrl+Shift+O)
- **Open Recent** > (子菜单，列出历史路径，外加 Clear Items)
- [Separator]
- **Save** (Ctrl+S)
- **Save As...** (Ctrl+Shift+S)
- [Separator]
- **Close** (Ctrl+W - 关闭当前 Tab)
- **Quit** (Ctrl+Q)

## 3. 实施步骤
1. **完善 IO 拦截**：增加“另存为”对话框调用逻辑 (`prompt_for_new_path`)。
2. **状态升维**：在 `SolaRoot` 新增 `recent_paths` 和 `active_submenu` 状态。
3. **级联渲染器**：重构 `render_menu_overlay`，使其支持渲染右侧的侧边弹出菜单（Submenu Overlay）。
4. **验证**：确保各项菜单能够准确触发相应操作，且子菜单的悬浮交互自然流畅，符合桌面软件直觉。
