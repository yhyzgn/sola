# Sola 实施工作日志

## 2026-04-23

1. 完成仓库初查：确认当前项目仅有最小化 `Cargo.toml` 与 `src/main.rs`，设计文档位于 `design/sola-architecture-design.md`。
2. 确认本轮首个实施边界：优先落地 Phase 1 基础设施与 Phase 2 的最小可运行原型，即：
   - Cargo workspace 化；
   - gpui 应用入口；
   - 文档模型与块级解析；
   - 基础主题系统；
   - Focused / Blurred 双态块渲染原型。
3. 参考 GPUI 官方 README / docs.rs，确认采用 `Application::new()` + 窗口根视图的组织方式，并按 workspace 方式拆分 crate。
4. 产出规划与测试规格：
   - `.codex/01-planning.md`
   - `.omx/plans/prd-sola-gpui-bootstrap.md`
   - `.omx/plans/test-spec-sola-gpui-bootstrap.md`
5. 完成 workspace 与 crate 拆分：
   - 根 `Cargo.toml` 改为 workspace，并引入 `[workspace.dependencies]`
   - 新增 `crates/sola-app`
   - 新增 `crates/sola-core`
   - 新增 `crates/sola-document`
   - 新增 `crates/sola-theme`
6. 完成首轮代码实现：
   - `sola-document`：基础 Markdown 块模型、focused block 状态、outline 构建、单元测试
   - `sola-theme`：结构化主题模型、TOML 解析、颜色解析、单元测试
   - `sola-app`：GPUI 窗口启动、shell 布局、侧栏、双态块渲染原型
   - 根 `src/main.rs`：仅作为 app 启动入口
7. 架构复审结论已吸收：
   - 保持 `sola-document` UI 无关
   - 保持 `sola-core` 极瘦
   - 本轮暂不做 HTML adapter
8. 验证完成：
   - `cargo fmt --all`
   - `cargo check`
   - `cargo test --workspace`
9. 根据用户更正，将工作记忆目录从 `./codex` 迁移为 `./.codex`，并同步修正文档引用。
10. 修复 `cargo run` 在当前 Linux 容器环境下的 GPUI 启动 panic：
   - 原因：Wayland / X11 后端均不可达，GPUI 启动时 panic
   - 处理：在 `crates/sola-app/src/shell.rs` 增加 Linux 显示后端可达性探测
   - 结果：当前环境下改为输出提示并干净退出，不再 panic
11. 继续推进下一步原型增强：
   - `sola-theme` 新增 `sola_light` 主题
   - `sola-document` 新增 `block_count`、`focused_block_ref`、`focus_next`、`focus_previous`
   - `sola-app` 新增主题切换按钮与 focused block 前后导航按钮
   - 在原型头部显示当前 theme 与 focused block 摘要
12. 本轮修改后再次完成强校验：
   - `cargo fmt --all`
   - `cargo check`
   - `cargo test --workspace`
   - `timeout 10s cargo run`
13. 推进 focused block 的“可编辑源码态雏形”：
   - `sola-document` 为 block 增加 `draft`
   - 支持 `set_focused_draft` / `append_to_focused_draft` / `revert_focused_draft` / `apply_focused_draft`
   - apply 后自动重建 `source` / `outline` / `stats`
   - `sola-app` 为 focused block 增加 `append draft note` / `revert draft` / `apply draft` 控件
14. 本轮修改后再次完成强校验：
   - `cargo fmt --all`
   - `cargo check`
   - `cargo test --workspace`
   - `timeout 10s cargo run`
