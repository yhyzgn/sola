# Sola Phase 5 Stage 3: IO 闭环与递归文件树计划

## 1. 目标
完善 Sola 作为真实编辑器的能力：实现本地文件的保存机制（IO 闭环），并将侧边栏文件树升级为支持深层级结构的递归扫描模型。

## 2. 核心设计

### A. 递归文件树 (`worktree.rs`)
当前的 `Worktree` 只有一层扁平扫描。我们需要利用之前已引入的 `ignore` crate 实现高性能的递归扫描：
- 修改 `Entry` 数据结构，使其支持嵌套：`pub children: Option<Vec<Entry>>`。
- 修改 `scan` 方法，使用 `ignore::WalkBuilder` 构建递归遍历器，自动过滤掉 `.gitignore` 中指定的构建产物和噪音目录（如 `target/`）。
- 这样不仅扫描速度极快，还能保证内存中持有完整的、过滤过的项目结构树。

### B. 树形 UI 渲染 (`project_panel.rs`)
- 重构 `collect_visible` 方法：遍历深层级的 `Entry`。
- 只有当当前 `Entry::is_dir` 并且 `self.expanded_dirs.contains(&entry.path)` 时，才继续递归遍历其 `children`。
- 根据递归深度（`depth`）渲染不同的缩进宽度。

### C. IO 保存闭环 (`workspace.rs` & `shell.rs`)
- 在 `Workspace` 实体中增加 `save_current_file(&mut self, cx: &mut Context<Self>)` 方法。
- 当 `self.current_path` 存在时，通过 `std::fs::write` 异步或同步地将 `self.document.source()` 写入磁盘。
- （可选）可以触发 `DocumentChanged` 以外的新事件如 `Saved` 来展示提示，或者利用现有的编辑历史标记（清除 draft 状态）。
- 在 `shell.rs` 的快捷键监听 (`handle_focused_key_down`) 中，当识别到 `Ctrl-S` / `Cmd-S` 且 `primary == true` 时，调用 `workspace.save_current_file()`。

## 3. 实施步骤
1. **重构 Worktree**：升级 `Entry` 为树形结构，利用 `ignore` 库完成深层扫描。
2. **升级 ProjectPanel**：更新虚拟列表的数据展平逻辑（`collect_visible`），使其支持嵌套目录的正确缩进与展开。
3. **实现保存逻辑**：在 `Workspace` 增加 IO 写入逻辑，并在 `shell.rs` 绑定 `Cmd/Ctrl+S`。
4. **编译与验证**：打开真实的多层级 Rust 项目，测试侧边栏的深层展开、文件读取以及编辑后的保存功能。
