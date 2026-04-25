# Sola 文件系统接入与侧边栏计划 (Phase 5) - GPUI Idiomatic

## 1. 目标
参考 Zed 的架构设计，将 Sola 从单体应用重构为 Model-View 分离的现代化编辑器。接入本地文件系统，实现高性能的虚拟化侧边栏文件树，并支持异步文件读取与后台变更监听。

## 2. 核心架构设计 (对齐 Zed 最佳实践)

### Model 层 (数据与业务逻辑)
- **`Worktree` (Entity)**：
  - 核心职责：映射本地磁盘目录。
  - 数据结构：维护文件/目录的层级树。
  - 监听机制：集成 `notify` crate，在后台线程监听文件变更。发生变更时，更新内部状态并通过 `cx.emit` 广播事件。
  - 快照机制：对外提供轻量级的 `Snapshot`，供 UI 线程无锁读取。
- **`Workspace` (Entity)**：
  - 充当全局上下文，持有当前的 `Worktree`、`DocumentModel` 和应用配置（如主题）。

### View 层 (UI 渲染)
- **`ProjectPanel` (Entity)**：
  - 侧边栏视图。不保存文件数据，仅维护 UI 状态（展开/折叠的目录集合）。
  - **虚拟化渲染**：使用 GPUI 的 `uniform_list` 组件。根据 `Worktree` 的状态和自身的折叠状态，计算出一个展平的“可见项”列表（如：0=src(展开), 1=main.rs, 2=lib.rs），按需渲染。
  - **事件订阅**：通过 `cx.subscribe` 监听 `Worktree` 的更新事件，触发视图重绘。
- **`WorkspaceView` (Entity)**：
  - 顶层视图壳层（Shell），负责布局 `ProjectPanel` 和右侧的编辑器主体。

## 3. 分步实施计划

### 第一步：抽象 Model 层 (`Worktree` 与 `Workspace`)
1. 引入 `notify` 和 `ignore`（处理隐藏文件）依赖。
2. 在 `sola-app` 中新建 `worktree.rs`，实现 `Worktree` Entity 及其后台扫描与监听逻辑。
3. 新建 `workspace.rs`，实现 `Workspace` Entity，将原 `SolaRoot` 中的数据状态剥离出来。

### 第二步：重构 View 层 (`ProjectPanel` 与 `WorkspaceView`)
1. 新建 `project_panel.rs`，实现侧边栏 Entity。
2. 引入 `uniform_list`，实现文件树的虚拟化绘制。处理目录的展开/折叠交互逻辑。
3. 改造 `shell.rs` 为 `WorkspaceView`，将 `ProjectPanel` 和现有的 `FocusedEditorElement` 组合起来。

### 第三步：打通 IO 闭环 (打开与保存)
1. 在 `ProjectPanel` 中处理文件的点击事件。
2. 使用 `cx.spawn` 和 `background_executor` 异步读取选中的 Markdown 文件。
3. 读取完成后，重置 `Workspace` 中的 `DocumentModel` 并刷新编辑器。
4. 注册全局快捷键 `Cmd-S` / `Ctrl-S`，实现将编辑器内容异步写回磁盘。

## 4. 预期收益
- **极致性能**：借助 `uniform_list`，即使打开包含数万文件的 Linux 内核源码目录，侧边栏也不会卡顿。
- **高响应度**：后台异步监听与读取，保证主 UI 线程永远不被文件 IO 阻塞。
- **可扩展性**：清晰的 Model-View 架构为后续实现多标签页（Tabs）、全局搜索（Search）等高级功能奠定了坚实的基础。
