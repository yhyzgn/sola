# 修复 Sola 应用文件树深度扫描导致的 UI 假死计划

## 1. 症状复发
在引入 `ignore::WalkBuilder` 进行递归目录树扫描后，`cargo run` 启动应用时再次出现了 10~20 秒的 "Not Responding" 假死状态，并且在文件发生大量变更时也会出现明显的卡顿。

## 2. 根因分析
在 `Worktree::local` 的初始化和事件分发循环中，调用了 `this.scan()` 方法。
```rust
handle.update(cx, |this, _| this.scan()); // 致命错误：在 UI 主线程执行同步重型 IO 扫描
```
虽然我们之前已经将 `notify` 监听器的系统级注册放到了后台，但是构建内存中 `Entry` 树的过程（包含数十到上千次磁盘访问和字符串处理）依然在 UI 线程同步阻塞执行。

## 3. 彻底的异步化修复方案
我们必须保证主线程中不包含任何与 `std::fs` 或同步遍历相关的调用。

### A. 剥离扫描逻辑
- 将 `scan` 方法从修改 `self` 内部状态的实例方法，重构为一个无状态的独立后台函数（例如 `scan_background_task(root_path: PathBuf) -> Option<Entry>`）。
- 这个函数负责执行所有耗时的 `WalkBuilder` 操作并组装完整的树。

### B. 后台派发与回调更新
- **初始加载**：在 `Worktree::local` 中，不再直接调用 `this.scan()`。改为使用 `cx.background_executor().spawn()` 启动后台扫描任务。
- **变更事件**：在收到 `rx.recv()` 的通知后，也不再直接更新 `Worktree`，而是重新触发后台扫描任务。
- **回流主线程**：在后台任务的最后，通过 `weak_handle.update` 将构建好的根节点 `root_entry` 安全地写回 `Worktree`，并调用 `cx.emit(WorktreeEvent::Updated)` 触发界面刷新。

## 4. 实施步骤
1. 打开 `crates/sola-app/src/worktree.rs`。
2. 提取 `scan` 和 `build_tree` 为静态辅助函数。
3. 修改 `Worktree::local` 的初始化流程，移除同步扫描，使用 `background_executor.spawn` 派发任务。
4. 修改 `notify` 事件处理循环，将每次更新时的同步扫描替换为触发同样的后台扫描流程。
5. 验证：启动应该实现真正的“毫秒级”，侧边栏在后台扫描完成后自动弹出文件树。
