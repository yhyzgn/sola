# 修复应用启动假死 (20s NRS) 计划

## 1. 问题描述
在应用启动阶段（`SolaRoot::new`），窗口会出现约 20 秒的 "Not Responding" 假死状态，随后恢复正常。

## 2. 根因分析
在 `Worktree::local` 的初始化逻辑中，调用了 `notify::RecommendedWatcher::watch(&abs_path, RecursiveMode::Recursive)`。
当 `abs_path` 为项目根目录时，该方法会在**当前线程（即 UI 主线程）同步地递归遍历整个目录树**（包括极其庞大的 `target/` 和 `.git/` 目录），以向操作系统注册文件系统监听句柄。这个同步 IO 密集型操作导致主线程长时间阻塞。

## 3. 修复方案
我们需要将文件系统的“重型 IO”操作从主线程彻底剥离，并引入必要的过滤机制。

### A. 异步化监听器注册
- 将 `watcher.watch(...)` 的调用放入 `cx.background_executor().spawn(...)` 中异步执行。
- 为了保持监听器的存活，当后台注册完成后，通过 `weak_handle.update` 将 `Watcher` 实例送回主线程，保存在 `Worktree` 的 `_watcher` 字段中。

### B. 过滤超大目录 (可选但推荐)
- 如果 `notify` 无法在监听层屏蔽 `target/`，我们需要在事件接收端快速丢弃来自 `target/` 和 `.git/` 的事件，防止频繁唤醒。
- 对于初期的 `scan` 操作（目前只是扁平扫描单层），确保它也是极快的，或者将其也移至后台。

## 4. 实施步骤
1. 修改 `crates/sola-app/src/worktree.rs`。
2. 提取 `watcher.watch` 调用，将其放入异步后台任务 (`cx.background_executor()`)。
3. 确保在等待后台注册期间，UI 能够秒级呈现。
4. 重构 `cx.spawn` 以接收真正的文件变更事件，并忽略构建目录的噪音。
