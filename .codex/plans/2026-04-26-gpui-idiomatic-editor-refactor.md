# Sola 主编辑区虚拟化重构计划 (Phase 5 - 极限性能)

## 1. 症状描述
用户反馈：即使将文件加载过程移至后台，打开长文件后依然会经历长达 10 秒的严重卡顿。这说明瓶颈并不在磁盘 I/O 或 Markdown 解析，而在于**视图层的全量渲染**。

## 2. 根因分析
目前的 `render_document_surface` 方法使用了极度低效的 `fold` 策略：
```rust
let blocks = document.blocks().iter().enumerate().fold(
    div(),
    |surface, (index, block)| surface.child(self.render_block(index, block, cx)),
);
```
这意味着如果一个文档有 500 个段落/公式/代码块，应用会在**每一帧**尝试构建和排版 500 个复杂的 DOM 子树！这在任何 UI 框架中都是灾难性的性能黑洞，是导致 10s 假死的绝对元凶。

## 3. 彻底修复方案

### A. 引入真正的 GPUI 虚拟列表 (Virtualization)
如同我们在 `ProjectPanel` 侧边栏中所做的那样，必须将主编辑区重构为 `gpui::list`：
- **按需渲染**：无论文档有多少个 `DocumentBlock`，只渲染屏幕内可见的几个块。内存占用从 $O(N)$ 降为 $O(1)$，渲染时间从数百毫秒降至不到 1 毫秒。
- **状态托管**：在 `SolaRoot` 或相关状态中维护 `gpui::ListState`。

### B. 优化 Typst 触发风暴 (Batch Updates)
在 `trigger_typst_renders` 中，对于已经存在于 `typst_cache` 中的数百个公式，之前的代码会触发几百个循环的 `cx.notify()`：
- **批量更新**：将循环内的 `cx.notify()` 移除。在收集完所有命中的缓存并更新完 `document` 后，在循环外统一调用一次 `cx.notify()`。

## 4. 实施步骤
1. **状态改造**：在 `SolaRoot` 或某个包装结构中引入 `ListState`（或者直接在 `render` 中构建无状态的 `gpui::list` 实例，如果不需要保持滚动状态）。*注意：为了保持正确的滚动位置，最好在切换 Tab 时更新或持有每个文档的滚动状态。如果追求最快修复，可先临时在渲染时构建。*
2. **重构 Surface**：彻底删除 `blocks` 的 `fold` 循环。用 `gpui::list` 包裹 `self.render_block` 调用。
3. **消除风暴**：重构 `trigger_typst_renders`，消灭高频同步重绘。
4. **终极验证**：用包含数百行内容的真实超大 Markdown 测试，验证能否真正实现瞬间打开（秒开）和 60 帧丝滑滚动。
