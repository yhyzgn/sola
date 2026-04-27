# Typora Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-architect the main document view to use a single `FocusedEditorElement` driven by GPUI's `TextSystem`, enabling pixel-perfect hit testing, cross-block selection, and Typora-style dual-state live preview.

**Architecture:** We are moving from a DOM-like `gpui::list` of distinct block `div`s to a unified text buffer. The `FocusedEditorElement` will map the entire `DocumentModel` into a continuous stream of `TextRun`s. It dynamically determines whether a block is in "Source Mode" (if the global cursor intersects it) or "Rich Text Mode" (otherwise) and styling the text runs accordingly. `TextSystem::shape_text` handles wrapping and hit-testing across the entire document.

**Tech Stack:** Rust, GPUI (`TextSystem`, `TextRun`, `WrappedLine`), `sola-document`

---

### Task 1: Global Cursor Support in DocumentModel

**Files:**
- Modify: `crates/sola-document/src/lib.rs`
- Test: `crates/sola-document/src/lib.rs` (in `mod tests`)

- [ ] **Step 1: Write the failing tests**

```rust
// in crates/sola-document/src/lib.rs (mod tests)
#[test]
fn test_global_cursor_conversion() {
    let mut doc = DocumentModel::from_markdown("# H1\n\nPara");
    assert_eq!(doc.source(), "# H1\n\nPara");
    
    // offset 6 is 'P'
    let (block_idx, local_offset) = doc.global_offset_to_block_local(6).unwrap();
    assert_eq!(block_idx, 1);
    assert_eq!(local_offset, 0);

    let global_offset = doc.block_local_to_global_offset(1, 0).unwrap();
    assert_eq!(global_offset, 6);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sola-document test_global_cursor_conversion`
Expected: FAIL with "method not found in `DocumentModel`"

- [ ] **Step 3: Write minimal implementation**

```rust
// in crates/sola-document/src/lib.rs (impl DocumentModel)
    pub fn global_offset_to_block_local(&self, global_offset: usize) -> Option<(usize, usize)> {
        let mut current_global = 0;
        for (i, block) in self.blocks.iter().enumerate() {
            let block_len = block.source.len();
            let block_end = current_global + block_len;
            
            if global_offset >= current_global && global_offset <= block_end {
                return Some((i, global_offset - current_global));
            }
            // +2 for the double newline between blocks (standardizing for now)
            // Or use the actual byte offsets if AST provides them.
            // Assuming current model joins with \n\n
            current_global = block_end + 2; 
        }
        None
    }

    pub fn block_local_to_global_offset(&self, block_index: usize, local_offset: usize) -> Option<usize> {
        let mut current_global = 0;
        for (i, block) in self.blocks.iter().enumerate() {
            if i == block_index {
                return Some(current_global + local_offset);
            }
            current_global += block.source.len() + 2; // +2 for \n\n
        }
        None
    }
```
*Note: Adjust the `+ 2` newline math based on how `source()` is reconstructed in `sola-document`.*

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sola-document test_global_cursor_conversion`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/sola-document/src/lib.rs
git commit -m "feat(document): add global to block local cursor offset conversions"
```

---

### Task 2: Implement Unified TextRun Generation

**Files:**
- Modify: `crates/sola-app/src/focused_editor.rs`
- Test: `crates/sola-app/src/focused_editor.rs`

- [x] **Step 1: Write the failing tests**

```rust
// in crates/sola-app/src/focused_editor.rs (mod tests)
#[test]
fn test_unified_text_run_generation() {
    let mut doc = DocumentModel::from_markdown("# H1\n\nText");
    let theme = Theme::sola_dark();
    let style = FocusedEditorStyle::from_theme(&theme);
    
    // Cursor at 0 (inside H1), H1 is Source, Text is Rich
    let runs = generate_unified_runs(&doc, Some(0), &style, &theme);
    
    // Total runs should cover "# H1\n\nText"
    // H1 in source mode will have multiple runs for syntax highlighting
    // Text in rich mode will be one or more runs with different typography
    assert!(!runs.is_empty());
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p sola-app test_unified_text_run_generation`
Expected: FAIL with "not found in this scope"

- [x] **Step 3: Write minimal implementation**

```rust
// in crates/sola-app/src/focused_editor.rs
use sola_document::DocumentModel;

pub fn generate_unified_runs(
    doc: &DocumentModel,
    global_cursor: Option<usize>,
    style: &FocusedEditorStyle,
    theme: &Theme,
) -> Vec<TextRun> {
    // ... (Implementation with dual-state live preview)
}
```

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p sola-app test_unified_text_run_generation`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/sola-app/src/focused_editor.rs
git commit -m "feat(editor): implement unified text run generation for dual-state live preview"
```

---

### Task 3: Migrate SolaRoot to Unified Editor Element

**Files:**
- Modify: `crates/sola-app/src/shell.rs`

- [ ] **Step 1: Replace gpui::list with FocusedEditorElement**

In `crates/sola-app/src/shell.rs`, inside `render_document_surface`:

```rust
// Remove the current gpui::list implementation
// let list = gpui::list(self.document_list_state.clone())...

// Generate unified text string
let full_text = doc.source().to_string();

// Calculate global cursor from active block (temporarily mapping old model to new)
// Note: This requires managing global cursor state in Workspace/SolaRoot later
let global_cursor = Some(0); // Placeholder for now, or calculate based on old focused_block

let runs = crate::focused_editor::generate_unified_runs(&doc, global_cursor, &editor_style, theme);

let editor_element = crate::focused_editor::FocusedEditorElement::new(
    full_text,
    editor_style,
    runs,
    Some(CursorState { anchor: None, head: global_cursor.unwrap_or(0) }),
    self.cursor_visible,
    gpui::rgb(sola_theme::parse_hex_color(&theme.palette.selection).unwrap_or(0x3e4451)).into(),
    gpui::rgb(sola_theme::parse_hex_color(&theme.palette.cursor).unwrap_or(0xffffff)).into(),
);

div()
    .id("document-surface")
    // ... layout classes
    .child(editor_element)
```

- [ ] **Step 2: Run cargo check**

Run: `cargo check -p sola-app`
Fix any resulting borrowing or structural issues in `shell.rs` as the UI layout changes.

- [ ] **Step 3: Commit**

```bash
git add crates/sola-app/src/shell.rs
git commit -m "refactor(editor): migrate main surface to unified FocusedEditorElement"
```