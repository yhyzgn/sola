# Sola 导出功能接入计划 (Phase 5 - 最后一块拼图)

## 1. 目标
目前的 `File -> Export` 菜单中虽然有了 HTML 和 Markdown 等选项，但它们只是空壳。我们已经拥有了基于 `pulldown_cmark` 的 `sola-export` Crate，需要将它正式接入到 Sola 的视图和交互层中，实现真实的“导出文件”到磁盘的功能。

## 2. 核心设计

### A. 依赖注入
修改 `crates/sola-app/Cargo.toml`，将 `sola-export` 作为依赖加入，使视图层能够调用底层的 `export_document` 方法。

### B. 交互链路 (SolaRoot)
在 `crates/sola-app/src/shell.rs` 中：
1. **新增辅助方法 `export_document`**：
   - 接收 `ExportFormat` 参数。
   - 获取当前活跃的 `DocumentModel` 及其文件名（用作默认保存名）。
   - 调用 `cx.prompt_for_new_path` 唤起系统“另存为”对话框。
   - 使用 `cx.background_executor().spawn` 在后台调用 `sola_export::export_document` 进行格式转换，避免阻塞 UI。
   - 将转换出的 `bytes` 通过 `std::fs::write` 异步写入目标路径。
2. **连接子菜单**：
   - 在 `render_cascading_submenu` 的 `"Export"` 分支中，将 "Markdown..." 和 "HTML..." 菜单项绑定到上述新方法上。
   - 移除不成熟的占位符（如果暂时不支持 PDF 导出，则可暂时移除或提示）。

### C. 文件名智能推断
- 如果当前文档有路径（已保存），导出的默认文件名应为 `原文件名.html`。
- 如果当前文档未保存（无路径），默认名称为 `untitled.html`。

## 3. 实施步骤
1. **更新 Cargo.toml**：注入依赖。
2. **实现方法**：在 `shell.rs` 中编写 `export_document_as` 方法。
3. **更新菜单**：在 `render_cascading_submenu` 挂载真实的 Action。
4. **编译与验证**：打开一个带公式和代码块的文档，点击 `Export -> HTML...`，选择路径保存并在浏览器中预览导出的结果，确认样式和数据完整无误。
