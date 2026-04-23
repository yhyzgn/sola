# 架构审查摘录（子代理审查结果）

时间：2026-04-23

## 结论

当前拆分方向基本合理，但需要保持以下约束：

1. `sola-document` 是核心，必须保持 **UI 无关**。
2. `sola-theme` 独立存在，后续可同时服务编辑渲染与导出。
3. `sola-core` 必须保持极瘦，避免演化为“公共杂物间”。
4. 本轮实施边界应收敛为 **Phase 1 + 精简版 Phase 2**，暂不引入 HTML adapter。

## 本轮吸收后的执行约束

- `sola-core` 仅保留稳定共享常量与样例内容，不承载复杂领域逻辑。
- `sola-document` 使用 typed model（`DocumentModel` / `DocumentBlock` / `BlockKind`），而非把 trait 当作领域核心。
- `pulldown-cmark` 仅作为首个 Markdown producer，用于 outline/结构信息抽取；后续 tree-sitter 作为 overlay 引入。
- 本轮不实现 HTML adapter、Tree-sitter、Typst、Mermaid、导出流水线。
