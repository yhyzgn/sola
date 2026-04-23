pub const APP_NAME: &str = "Sola";
pub const APP_TAGLINE: &str = "A GPUI-native Markdown editor aligned with Typora's writing flow.";

pub const ROADMAP_PHASES: [&str; 5] = [
    "Phase 1 · Workspace, app shell, editor skeleton",
    "Phase 2 · Dual-state blocks, HTML adapter, TOML theme",
    "Phase 3 · Tree-sitter, tables, image workflows",
    "Phase 4 · Typst, Mermaid, PDF and long-image export",
    "Phase 5 · Filesystem integration and platform polish",
];

pub fn sample_markdown() -> &'static str {
    r#"# Sola

Sola is a GPU-accelerated Markdown editor focused on immersive writing and structured rendering.

## Why this first prototype exists

- establish a workspace-first Rust architecture
- prove the GPUI application shell
- prototype focused / blurred block rendering

> The document is the source, and the source is the document.

```rust
fn focused_block() -> &'static str {
    "render markdown source directly when editing"
}
```

### Next milestones

1. Tree-sitter highlighting
2. Typst-powered formula rendering
3. Offline export pipeline
"#
}
