# Sola 修复打开文件卡死计划 (Phase 5 - 性能攻坚)

## 1. 症状描述
用户反馈在侧边栏点击打开任意 `.md` 文件后，编辑器主区域会进入长时间的卡顿（无响应，NRS）状态，完全无法操作。

## 2. 根因分析
通过溯源代码，我们发现尽管之前修复了 `Worktree` 目录扫描的同步阻塞，但在**“打开单个文件”**的链路上，依然残留了致命的同步代码：
1. **同步磁盘 I/O**：在 `workspace.rs` 中，`open_file` 方法直接使用了 `std::fs::read_to_string`。
2. **同步 AST 解析**：`DocumentModel::from_markdown` 包含了大量的正则匹配、字符串拆分、HTML 转换与大纲提取逻辑。对于稍大一点的文档，在主线程执行这些计算会导致长达数百毫秒甚至数秒的 UI 阻塞。
3. **频繁的目录重建**：每次打开一个文件，系统都会尝试将其父目录作为新的根目录去调用 `Worktree::local`。如果该文件原本就处于当前项目中，这就引发了一次毫无必要的全盘扫描风暴，进一步拖垮了主线程。

## 3. 彻底修复方案

### A. 智能的工作树复用 (Smart Worktree)
在 `shell.rs` 的 `open_path` 中：
- 检查新打开的文件的父目录是否已经是当前 `Workspace` 的 `worktree` 的根目录。
- 如果**是**，则**跳过** `Worktree::local` 初始化，直接进入文件加载。
- 这样，在侧边栏连续点击切换文件时，将不再有任何目录扫描开销。

### B. 极限异步解析 (Off-thread Parsing)
将文件读取与模型构建彻底赶出 UI 线程：
- 在 `shell.rs` 捕获到打开文件请求后，使用 `cx.background_executor().spawn`。
- 在后台线程中：
  1. 调用 `std::fs::read_to_string`。
  2. 调用 `DocumentModel::from_markdown` 完成所有重型的文本解析和结构化工作。
- 只有当 `DocumentModel` 这个“成品”在后台构建完毕后，才通过异步回调投递回主线程的 `Workspace` 中进行轻量级的数据绑定。

### C. 模型接口改造
- 将 `Workspace::open_file` 的签名修改为 `pub fn open_file(&mut self, path: PathBuf, document: DocumentModel, cx: &mut Context<Self>)`，让它只负责极其轻量级的数据挂载。

## 4. 实施步骤
1. **修改 Workspace 模型**：在 `crates/sola-app/src/workspace.rs` 剥离 `std::fs` 和 `from_markdown` 的调用。
2. **重构 Shell 路由**：在 `crates/sola-app/src/shell.rs` 重写 `open_path`，实现后台读取解析与 Worktree 重用。
3. **编译并验证**：打开大型 Markdown 文件，验证应用能否保持 60 帧的丝滑响应，并在加载完毕后瞬间呈现内容。
