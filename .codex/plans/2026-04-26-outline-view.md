# Sola 大纲视图计划 (Phase 5 - 导航增强)

## 1. 目标
为长篇学术文档、长博客等使用场景提供“大纲导航（Table of Contents）”功能。用户可以在侧边栏快速切换到“大纲”模式，概览当前文档的所有标题，并一键跳转至对应位置。

## 2. 核心架构设计

### A. 标题数据源 (`sola-document`)
废弃原来基于全文 `pulldown-cmark` 重新解析的大纲逻辑。
转而利用已经结构化好的 `blocks` 序列：
- 增加 `DocumentModel::get_headings` 方法，直接遍历 `self.blocks`。
- 筛选出所有 `BlockKind::Heading` 的块。
- 提取并返回它们的 `(block_index, level, text)`。

### B. 侧边栏模式切换 (`ProjectPanel`)
在 `ProjectPanel` 的状态中新增模式追踪：
```rust
enum SidebarMode {
    Files,
    Outline,
}
```
在侧边栏的 Header 区域（目前写着 "PROJECT" 的地方）替换为两个并排的 Tab 按钮："FILES" 和 "OUTLINE"。点击即可切换 `self.mode`。

### C. 大纲渲染与交互
当 `mode == SidebarMode::Outline` 时：
1. **渲染**：调用活跃文档的 `get_headings()`，根据 `level` 动态计算左侧缩进（例如 `padding-left: level * 10px`）。
2. **跳转**：在标题项上绑定 `on_mouse_down`，点击后直接调用 `workspace.update_active_document(cx, |doc| doc.focus_block(index))`，并触发重绘。
3. **空状态**：如果当前无打开的文件，或者文档中没有标题，则显示友好的占位文案。

## 3. 实施步骤
1. **重构数据层**：修改 `crates/sola-document/src/lib.rs`，增加基于块遍历的 `get_headings` 方法。
2. **状态扩充**：在 `project_panel.rs` 中引入 `SidebarMode` 并初始化。
3. **UI 改造**：在 `ProjectPanel::render` 中添加顶部切换按钮，并根据模式分离列表渲染逻辑（文件树 / 大纲）。
4. **验证联动**：在编辑器中新增几个带有不同层级 `#` 的段落，观察大纲视图是否实时更新，点击大纲项光标是否正确跳转。
