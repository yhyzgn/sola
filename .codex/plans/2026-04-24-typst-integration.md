# Sola Typst Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate Typst math formula and block rendering into Sola with native crate integration and asynchronous background rendering.

**Architecture:** Create a new `sola-typst` crate for compiler logic, extend `sola-document` with new block types and adapters, and update `sola-app` to handle background compilation and SVG rendering.

**Tech Stack:** Rust, Typst (crate), GPUI, pulldown-cmark.

---

### Task 1: Initialize `sola-typst` Crate

**Files:**
- Create: `crates/sola-typst/Cargo.toml`
- Create: `crates/sola-typst/src/lib.rs`
- Modify: `Cargo.toml` (root)

- [ ] **Step 1: Create `crates/sola-typst/Cargo.toml`**

```toml
[package]
name = "sola-typst"
version = "0.1.0"
edition = "2021"

[dependencies]
typst = "0.13.0"
typst-svg = "0.13.0"
typst-assets = "0.13.0"
serde = { version = "1.0", features = ["derive"] }
thiserror = "1.0"
```

- [ ] **Step 2: Create `crates/sola-typst/src/lib.rs` with basic stubs**

```rust
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TypstError {
    #[error("Compilation failed: {0}")]
    Compile(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

pub enum RenderKind {
    Math,
    Block,
}

pub fn compile_to_svg(source: &str, kind: RenderKind) -> Result<String, TypstError> {
    // Stub implementation
    Ok("<svg></svg>".to_string())
}
```

- [ ] **Step 3: Register in root `Cargo.toml`**

```toml
[workspace]
members = [
    "crates/sola-app",
    "crates/sola-core",
    "crates/sola-document",
    "crates/sola-theme",
    "crates/sola-typst",
]
# ... rest of file
```

- [ ] **Step 4: Verify build**

Run: `cargo check -p sola-typst`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/sola-typst
git commit -m "feat: initialize sola-typst crate"
```

---

### Task 2: Implement Typst World and Font Provider

**Files:**
- Modify: `crates/sola-typst/src/lib.rs`

- [ ] **Step 1: Implement `SolaWorld` struct and required traits**

```rust
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::Library;
use typst::World;

struct SolaWorld {
    library: Library,
    book: FontBook,
    fonts: Vec<Font>,
    source: Source,
}

impl SolaWorld {
    fn new(text: &str) -> Self {
        let library = Library::default();
        let fonts: Vec<Font> = typst_assets::fonts()
            .map(|data| Font::new(Bytes::from(data), 0).unwrap())
            .collect();
        let book = FontBook::from_fonts(&fonts);
        let source = Source::detached(text);

        Self {
            library,
            book,
            fonts,
            source,
        }
    }
}

impl World for SolaWorld {
    fn library(&self) -> &Library { &self.library }
    fn book(&self) -> &FontBook { &self.book }
    fn main(&self) -> Source { self.source.clone() }
    fn source(&self, _id: FileId) -> FileResult<Source> { Ok(self.source.clone()) }
    fn binary(&self, _id: FileId) -> FileResult<Bytes> { Err(FileError::NotFound(_id.vpath().as_rootless_path().to_path_buf())) }
    fn font(&self, id: usize) -> Option<Font> { self.fonts.get(id).cloned() }
    fn today(&self, _offset: Option<i64>) -> Option<Datetime> { None }
}
```

- [ ] **Step 2: Update `compile_to_svg` to use `SolaWorld`**

```rust
pub fn compile_to_svg(source: &str, kind: RenderKind) -> Result<String, TypstError> {
    let full_source = match kind {
        RenderKind::Math => format!("#set page(width: auto, height: auto, margin: 0pt)\n${}$", source),
        RenderKind::Block => source.to_string(),
    };

    let world = SolaWorld::new(&full_source);
    let mut tracer = typst::diag::Tracer::default();
    let document = typst::compile(&world, &mut tracer)
        .map_err(|err| TypstError::Compile(format!("{:?}", err)))?;

    let svg = typst_svg::svg(&document.pages[0].frame);
    Ok(svg)
}
```

- [ ] **Step 3: Verify build**

Run: `cargo check -p sola-typst`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/sola-typst/src/lib.rs
git commit -m "feat: implement Typst World and SVG compilation"
```

---

### Task 3: Extend `sola-document` Model

**Files:**
- Modify: `crates/sola-document/src/lib.rs`

- [ ] **Step 1: Add `TypstAdapter` and update `BlockKind`**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypstAdapter {
    Pending,
    Rendered { svg: String },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    // ... existing
    MathBlock,
    TypstBlock,
}
```

- [ ] **Step 2: Update `DocumentBlock` struct**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentBlock {
    // ... existing
    pub typst: Option<TypstAdapter>,
}
```

- [ ] **Step 3: Update `new_block` and `rebuild_metadata` to initialize `typst` field**

- [ ] **Step 4: Update parser to recognize `$$` and ` ```typst `**

```rust
// In parse_blocks
if line.trim_start().starts_with("$$") {
    // Handle math block
}
if let Some(info) = line.trim_start().strip_prefix("```") {
    if info.trim() == "typst" {
        // Handle typst block
    }
}
```

- [ ] **Step 5: Verify tests pass (ignoring new features)**

Run: `cargo test -p sola-document`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/sola-document/src/lib.rs
git commit -m "feat: extend document model for Typst and Math blocks"
```

---

### Task 4: Integrate Background Rendering in `sola-app`

**Files:**
- Modify: `crates/sola-app/Cargo.toml`
- Modify: `crates/sola-app/src/shell.rs`

- [ ] **Step 1: Add `sola-typst` dependency to `sola-app`**

- [ ] **Step 2: Implement asynchronous rendering trigger in `SolaRoot`**

```rust
// In render_document_surface or after focus change
fn trigger_typst_renders(&mut self, cx: &mut Context<Self>) {
    // Iterate blocks, if kind is Math/Typst and state is Pending or needs update
    // Spawn background task
}
```

- [ ] **Step 3: Update `render_block_card` to handle `TypstAdapter` states**

```rust
// If block.typst is some
match typst {
    TypstAdapter::Pending => div().child("Loading formula..."),
    TypstAdapter::Rendered { svg } => gpui::svg().path(svg).size_full(),
    TypstAdapter::Error { message } => div().text_color(red).child(message),
}
```

- [ ] **Step 4: Verify with a sample markdown containing math**

- [ ] **Step 5: Commit**

```bash
git add crates/sola-app
git commit -m "feat: integrate background Typst rendering and SVG display"
```

---

### Task 5: Final Validation and Polish

- [ ] **Step 1: Run all workspace tests**
- [ ] **Step 2: Verify `cargo run` looks good with math and typst blocks**
- [ ] **Step 3: Update `.codex` worklog and status**
- [ ] **Step 4: Commit**
