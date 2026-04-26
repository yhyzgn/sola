# Sola 多标签页 (Multi-tab) 系统计划 (Phase 5 - 扩展)

## 1. 目标
目前的 Sola 在打开新文件时，会直接覆盖当前的单例文档。为了提供现代编辑器的标配体验，我们需要引入多标签页系统，允许用户同时打开、在内存中保持编辑状态，并在多个 Markdown 文件之间无缝切换。

## 2. 核心架构设计

### A. 模型层升级 (`workspace.rs`)
在 `Workspace` 实体中，将现有的单一 `document` 和 `current_path` 状态替换为缓冲池集合：
- `open_documents: Vec<(Option<PathBuf>, DocumentModel)>`：管理当前所有打开的文件缓冲（Buffer）。
- `active_item_index: Option<usize>`：记录当前选中的标签页索引。
- 新增管理方法：
  - `open_file`：检查目标文件是否已在池中，若在则直接将其置为活跃状态（Active）；否则读取磁盘内容，插入新标签页并切换焦点。
  - `close_tab(index)`：关闭指定标签页，若关闭的是当前活跃的 Tab，则智能回退焦点至相邻 Tab；若全关则显示空状态。
  - `switch_tab(index)`：切换活跃 Tab。
  - `active_document()` 和 `active_document_mut()`：提供安全的方式返回当前获得焦点的文档句柄。

### B. 视图层适配 (`shell.rs` & `project_panel.rs`)
1. **渲染标签栏 (Tab Bar)**：
   - 在 `render_document_surface` 或顶部区域新增 `flex_row`，用于渲染所有的 Tab。
   - 为 Tab 赋予不同的视觉状态：未激活态（较低透明度的背景色）与激活态（与主编辑区融为一体的背景色）。
   - 在 Tab 上渲染文件名称，并提供关闭按钮（`x`）。
2. **事件路由更新**：
   - 将原先调用 `workspace.document_mut()` 的代码重构为 `workspace.active_document_mut()`。
   - 若 `active_document` 为空，则编辑器主区域显示“Sola 空白占位页”或大 Logo，并拦截相关键盘事件。

### C. 快捷键与 Action 扩展
- 新增 `CloseTab` Action：绑定 `Cmd/Ctrl+W`。
- 新增 `NextTab` / `PrevTab` Action：支持键盘快速切换。
- 保存操作 (`Cmd/Ctrl+S`) 自动路由至当前 `active_document` 对应的物理路径。

## 3. 实施步骤
1. **重构 Workspace 模型**：在不破坏现有外部 API 接口的前提下，把底层数据结构升级为向量（Vec），实现基础的 Tab 增删改查。
2. **升级 View UI**：在 `shell.rs` 中编写 `render_tab_bar` 函数，支持点击切换和悬浮关闭。
3. **处理空状态**：处理所有 Tab 均被关闭时的回退逻辑，渲染默认引导页。
4. **编译与验证测试**：测试多个文件之间的相互隔离情况，尤其是各自的 Typst 公式渲染队列、撤销栈（Undo History）以及未保存草稿的独立性。
