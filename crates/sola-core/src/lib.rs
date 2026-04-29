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
    r##"# Sola Editor

Sola is a **GPU-accelerated** Markdown editor focused on *immersive writing* and ~~structured~~ precise rendering.

## 1. Typography & Inline Styles

You can use **Bold**, *Italic*, or ***Bold Italic***.  
~~Strikethrough~~ is also supported, as well as `inline code` for technical terms.

Links like [Sola on GitHub](https://github.com/obra/sola) are rendered cleanly in Rich Mode.

## 2. Lists & Task Management

### Task List
- [x] Implement Unified Canvas
- [x] Smooth Source/Rich transition
- [ ] Multi-format Export (PDF, Image)
- [ ] File System Tree

### Ordered List
1. First priority item
2. Second sequential task
3. Third follow-up action

### Unordered & Nested
- Level 1 Item
    - Nested Level 2
    - Another Level 2
- Back to Level 1

## 3. Quotes & Code

> "The document is the source, and the source is the document."
> 
> — Sola Philosophy

```rust
fn main() {
    println!("Hello from Sola's Syntax Highlighter!");
    let editor = "FocusedEditorElement";
}
```

## 4. Mathematics & Typst

Inline math like $e^{i \pi} + 1 = 0$ is rendered via Typst.

Display math:
$$
\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}
$$

```typst
#set text(fill: rgb("#8b5cf6"))
#align(center)[
  *This is a Typst-native block*
  $ cal(A) = pi r^2 $
]
```

## 5. HTML & Images

<span style="color: #ff7a59; font-size: 18px">Warm inline emphasis</span> via HTML is supported.

Images show placeholders with alt text:
![Architecture of Sola](architecture-sketch.png)
"##
}
