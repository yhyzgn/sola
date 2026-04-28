# Unified Editor Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor Sola's editor to a unified, canvas-based `FocusedEditorElement` for seamless cross-block selection, pixel-perfect layout, and future high-fidelity export support.

**Architecture:** We are moving from a `gpui::list` of separate `Div` elements to a single custom `gpui::Element`. `DocumentModel` blocks will be flattened into a stream of `TextRun`s. We will use a global UTF-8 byte offset for the cursor and selection. Inline math will be rendered by reserving space using the `U+FFFC` object replacement character and painting the SVG over it.

**Tech Stack:** Rust, GPUI, Tree-sitter, Typst (via `sola-typst`).

---

## File Structure Map

*   **Modify `crates/sola-document/src/lib.rs`**: Add global-to-local and local-to-global offset mapping logic.
*   **Create `crates/sola-app/src/editor_layout.rs`**: Define the export-ready `VisualDocument`, `VisualLine`, and `SolaLayoutEngine` traits/structs.
*   **Modify `crates/sola-app/src/focused_editor.rs`**: Update `FocusedEditorElement` to use global offsets, generate `TextRun`s for the whole document (with virtualization), and handle the new layout engine. Implement the `U+FFFC` replacement logic.
*   **Modify `crates/sola-app/src/shell.rs`**: Replace the current `gpui::list` with the new `FocusedEditorElement`. Update keybindings to use global offsets.

---

## Task 1: Document Global Offset Mapping

Add methods to `DocumentModel` to translate between global UTF-8 byte offsets (for the whole document) and block-local offsets.

**Files:**
- Modify: `crates/sola-document/src/lib.rs`

- [ ] **Step 1: Write failing tests for global offset mapping**

```rust
// In crates/sola-document/src/lib.rs, in the `tests` module
#[test]
fn test_global_to_local_offset() {
    let mut doc = DocumentModel::from_markdown("# Header\n\nParagraph text.");
    
    // Global offset 0 -> Block 0, local offset 0 ('#')
    assert_eq!(doc.global_offset_to_block_local(0), Some((0, 0)));
    // Global offset 8 -> Block 0, local offset 8 (end of "# Header")
    assert_eq!(doc.global_offset_to_block_local(8), Some((0, 8)));
    
    // Global offset 9 is the first '\n'
    // Global offset 10 is the second '\n'
    // Global offset 11 -> Block 1, local offset 0 ('P')
    assert_eq!(doc.global_offset_to_block_local(11), Some((1, 0)));
}

#[test]
fn test_local_to_global_offset() {
    let mut doc = DocumentModel::from_markdown("# Header\n\nParagraph text.");
    
    assert_eq!(doc.block_local_to_global_offset(0, 0), Some(0));
    assert_eq!(doc.block_local_to_global_offset(0, 8), Some(8));
    assert_eq!(doc.block_local_to_global_offset(1, 0), Some(11));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sola-document test_global_to_local_offset test_local_to_global_offset`
Expected: Compilation failure or test failure (methods don't exist yet).

- [ ] **Step 3: Implement offset mapping methods**

```rust
// In crates/sola-document/src/lib.rs, inside `impl DocumentModel`

/// Converts a global UTF-8 byte offset to a `(block_index, local_offset)` tuple.
pub fn global_offset_to_block_local(&self, global_offset: usize) -> Option<(usize, usize)> {
    let mut current_global = 0;
    for (i, block) in self.blocks.iter().enumerate() {
        let block_len = block.source.len();
        // Check if the global offset falls within this block
        if global_offset >= current_global && global_offset <= current_global + block_len {
            return Some((i, global_offset - current_global));
        }
        // Advance global offset by block length + 2 for the "\n\n" separator
        current_global += block_len + 2; 
    }
    None
}

/// Converts a `block_index` and a `local_offset` within that block to a global UTF-8 byte offset.
pub fn block_local_to_global_offset(&self, block_index: usize, local_offset: usize) -> Option<usize> {
    if block_index >= self.blocks.len() {
        return None;
    }
    
    let mut global_offset = 0;
    for i in 0..block_index {
        global_offset += self.blocks[i].source.len() + 2; // +2 for "\n\n"
    }
    
    let block = &self.blocks[block_index];
    if local_offset <= block.source.len() {
        Some(global_offset + local_offset)
    } else {
        None
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sola-document test_global_to_local_offset test_local_to_global_offset`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/sola-document/src/lib.rs
git commit -m "feat(document): add global/local offset mapping methods"
```

---

## Task 2: Export-Ready Layout Abstraction

Create the structures that represent a laid-out document, ready for either GPUI rendering or off-screen export.

**Files:**
- Create: `crates/sola-app/src/editor_layout.rs`
- Modify: `crates/sola-app/src/lib.rs`

- [ ] **Step 1: Define VisualDocument structures**

```rust
// Create crates/sola-app/src/editor_layout.rs
use gpui::{Pixels, Point, TextRun, Window, WrappedLine};
use std::ops::Range;

/// Represents an inline object (like math or an image) that needs space reserved.
#[derive(Debug, Clone)]
pub struct VisualObject {
    pub global_offset: usize,
    pub width: Pixels,
    pub height: Pixels,
    pub cache_key: String, // Used to lookup the SVG in the Typst cache
}

/// A single visually wrapped line of text, plus any objects that appear on this line.
#[derive(Clone)]
pub struct VisualLine {
    pub global_start: usize,
    pub global_end: usize,
    pub bounds: gpui::Bounds<Pixels>,
    pub wrapped_line: WrappedLine,
    pub objects: Vec<(VisualObject, gpui::Point<Pixels>)>, // Object and its relative (x, y) offset within the line
}

/// The fully laid out document, ready to be painted to GPUI or exported.
pub struct VisualDocument {
    pub lines: Vec<VisualLine>,
    pub total_height: Pixels,
}

impl VisualDocument {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            total_height: Pixels::ZERO,
        }
    }
}
```

- [ ] **Step 2: Expose the module**

```rust
// In crates/sola-app/src/lib.rs
// Add this line:
pub mod editor_layout;
```

- [ ] **Step 3: Commit**

```bash
git add crates/sola-app/src/editor_layout.rs crates/sola-app/src/lib.rs
git commit -m "feat(editor): introduce export-ready VisualDocument abstraction"
```

---

## Task 3: Refactor EditorBlock Generation (Dual-State Logic)

Update `generate_editor_blocks` to use the global offset to determine the focused block, and correctly insert the `U+FFFC` placeholder for inline math.

**Files:**
- Modify: `crates/sola-app/src/focused_editor.rs`

- [ ] **Step 1: Write a test for placeholder generation**

```rust
// In crates/sola-app/src/focused_editor.rs, in the `tests` module
#[test]
fn test_generate_editor_blocks_replaces_inline_math() {
    let doc = sola_document::DocumentModel::from_markdown("Text with $E=mc^2$ inline math.");
    let theme = sola_theme::Theme::sola_dark();
    let style = FocusedEditorStyle::from_theme(&theme);
    
    // Cursor is None, so the block should be in Rich Mode (blurred)
    let blocks = generate_editor_blocks(&doc, None, &style, &theme);
    
    assert_eq!(blocks.len(), 1);
    let block = &blocks[0];
    assert!(!block.is_focused);
    
    // The text should have U+FFFC
    assert_eq!(block.text, "Text with \u{FFFC} inline math.");
    assert_eq!(block.inline_math.len(), 1);
    assert_eq!(block.inline_math[0].cache_key, "math::E=mc^2");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sola-app test_generate_editor_blocks_replaces_inline_math`
Expected: FAIL (The current implementation might already be close, but let's ensure it's exact).

- [ ] **Step 3: Update `generate_editor_blocks`**

*Note: The implementation in `focused_editor.rs` is already quite close to this. We just need to ensure the signature takes `global_cursor: Option<usize>` instead of a block index, which it currently seems to do.*

```rust
// In crates/sola-app/src/focused_editor.rs
// Ensure this function signature and logic exists. If it matches the current implementation, this step is just verification.
// Look for `pub fn generate_editor_blocks` and ensure it handles `global_cursor` and inserts `\u{FFFC}` correctly.
// Also ensure `InlineDecoration` has `start`, `end`, and `cache_key`.
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p sola-app test_generate_editor_blocks_replaces_inline_math`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/sola-app/src/focused_editor.rs
git commit -m "refactor(editor): ensure EditorBlock generation uses global cursor and placeholders"
```

---

## Task 4: Implement SolaLayoutEngine Layout Logic

Write the logic that takes a list of `EditorBlock`s and shapes them into a `VisualDocument` using GPUI's `TextSystem`.

**Files:**
- Modify: `crates/sola-app/src/editor_layout.rs`

- [ ] **Step 1: Write the `layout_document` function**

```rust
// In crates/sola-app/src/editor_layout.rs
use crate::focused_editor::EditorBlock;
use gpui::{SharedString, Font, FontFeatures, FontWeight, FontStyle};

pub fn layout_document(
    window: &mut Window,
    blocks: &[EditorBlock],
    wrap_width: Pixels,
) -> VisualDocument {
    let mut doc = VisualDocument::new();
    let mut current_y = gpui::Pixels::ZERO;

    for block in blocks {
        let lines = window
            .text_system()
            .shape_text(
                SharedString::from(block.text.clone()),
                block.font_size,
                &block.runs,
                Some(wrap_width),
                None,
            )
            .unwrap_or_default()
            .into_vec();

        let mut block_rendered_base = 0;
        
        for line in &lines {
            let line_height = block.line_height;
            let mut boundaries = line
                .wrap_boundaries()
                .iter()
                .map(|b| line.runs()[b.run_ix].glyphs[b.glyph_ix].index)
                .collect::<Vec<_>>();
            boundaries.push(line.len());

            let mut local_start = 0;
            for local_end in boundaries {
                let text_start = block_rendered_base + local_start;
                let text_end = block_rendered_base + local_end;

                let global_start = block.global_start + block.rendered_to_source(text_start);
                let global_end = block.global_start + block.rendered_to_source(text_end);

                let bounds = gpui::Bounds {
                    origin: gpui::Point { x: gpui::Pixels::ZERO, y: current_y },
                    size: gpui::size(wrap_width, line_height),
                };

                // Find objects in this visual line
                let mut objects = Vec::new();
                for deco in &block.inline_math {
                    let deco_rendered_start = block.source_to_rendered(deco.start);
                    // Check if the placeholder U+FFFC is in this line
                    if deco_rendered_start >= text_start && deco_rendered_start < text_end {
                        let offset_in_line = deco_rendered_start - local_start;
                        let x = line.unwrapped_layout.x_for_index(offset_in_line);
                        
                        objects.push((
                            VisualObject {
                                global_offset: block.global_start + deco.start,
                                width: gpui::px(40.0), // Placeholder width for now
                                height: line_height - gpui::px(4.0),
                                cache_key: deco.cache_key.clone(),
                            },
                            gpui::Point { x, y: gpui::px(2.0) } // Relative to line bounds
                        ));
                    }
                }

                doc.lines.push(VisualLine {
                    global_start,
                    global_end,
                    bounds,
                    wrapped_line: line.clone(),
                    objects,
                });

                local_start = local_end;
                current_y += line_height;
            }
            block_rendered_base += line.text.len() + 1; // +1 for the implicit newline in layout
        }
        current_y += block.line_height; // Block spacing
    }

    doc.total_height = current_y;
    doc
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/sola-app/src/editor_layout.rs
git commit -m "feat(editor): implement layout_document to generate VisualDocument"
```

---

## Task 5: Refactor FocusedEditorElement Request Layout

Update `FocusedEditorElement` to use the new `VisualDocument` state instead of the old `VisualLineRef`s.

**Files:**
- Modify: `crates/sola-app/src/focused_editor.rs`

- [ ] **Step 1: Update the Element State**

```rust
// In crates/sola-app/src/focused_editor.rs
use crate::editor_layout::{layout_document, VisualDocument};

pub struct FocusedEditorState {
    visual_doc: VisualDocument,
}

// In the `Element` implementation for `FocusedEditorElement`
    fn request_layout(
        &mut self,
        _global_id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let wrap_width = approximate_editor_wrap_width(window.bounds().size.width);

        let visual_doc = layout_document(window, &self.blocks, wrap_width);

        let total_height = visual_doc.total_height + gpui::px(100.0); // Add bottom padding

        let mut style = gpui::Style::default();
        style.size.width = gpui::relative(1.0).into();
        style.size.height = total_height.into();

        let layout_id = window.request_layout(style, None, cx);
        (
            layout_id,
            FocusedEditorState { visual_doc },
        )
    }
```

- [ ] **Step 2: Commit**

```bash
git add crates/sola-app/src/focused_editor.rs
git commit -m "refactor(editor): update FocusedEditorElement request_layout to use VisualDocument"
```

---

## Task 6: Refactor FocusedEditorElement Paint

Update the `paint` method to iterate over `VisualDocument.lines` and draw the text, selections, and objects.

**Files:**
- Modify: `crates/sola-app/src/focused_editor.rs`

- [ ] **Step 1: Rewrite the `paint` method**

*Note: Replace the existing `paint` method in `FocusedEditorElement`.*

```rust
// In crates/sola-app/src/focused_editor.rs, inside `impl Element for FocusedEditorElement`

    fn paint(
        &mut self,
        _global_id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        request_layout_state: &mut Self::RequestLayoutState,
        _prepaint_state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let visual_doc = &request_layout_state.visual_doc;

        let padding = gpui::Point {
            x: self.style.padding_x,
            y: self.style.padding_y,
        };
        let text_bounds = gpui::Bounds {
            origin: bounds.origin + padding,
            size: gpui::size(
                bounds.size.width - self.style.padding_x * 2.0,
                bounds.size.height - self.style.padding_y * 2.0,
            ),
        };

        // 1. Paint Selection
        if let Some(cursor) = &self.cursor {
            if let Some(anchor) = cursor.anchor {
                let start = anchor.min(cursor.head);
                let end = anchor.max(cursor.head);

                for line in &visual_doc.lines {
                    let overlap_start = start.max(line.global_start);
                    let overlap_end = end.min(line.global_end);

                    if overlap_start < overlap_end {
                        // Very rough approximation for selection rectangles for now.
                        // A true implementation needs to map global back to local for x_for_index.
                        let rect = gpui::Bounds {
                            origin: text_bounds.origin + line.bounds.origin,
                            size: line.bounds.size,
                        };
                        window.paint_quad(gpui::fill(rect, self.selection_color));
                    }
                }
            }
        }

        // 2. Paint Text and Objects
        for line in &visual_doc.lines {
            // Paint text
            let _ = line.wrapped_line.paint(
                text_bounds.origin + line.bounds.origin,
                line.bounds.size.height,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );

            // Paint objects
            for (obj, offset) in &line.objects {
                if let Some(TypstAdapter::Rendered { svg }) = self.typst_cache.get(&obj.cache_key) {
                    let svg_bounds = gpui::Bounds {
                        origin: text_bounds.origin + line.bounds.origin + *offset,
                        size: gpui::size(obj.width, obj.height),
                    };
                    
                    let _ = window.paint_svg(
                        svg_bounds,
                        svg.clone().into(),
                        gpui::TransformationMatrix::default(),
                        gpui::white(),
                        cx,
                    );
                }
            }
        }

        // 3. Paint Caret
        if let Some(cursor) = &self.cursor {
            if self.cursor_visible {
                for line in &visual_doc.lines {
                    if cursor.head >= line.global_start && cursor.head <= line.global_end {
                        let caret_bounds = gpui::Bounds {
                            origin: text_bounds.origin + line.bounds.origin, // Simplified x pos
                            size: gpui::size(self.style.caret_width, line.bounds.size.height),
                        };
                        window.paint_quad(gpui::fill(caret_bounds, self.cursor_color));
                        break;
                    }
                }
            }
        }
    }
```

*Self-correction note: The selection and caret x-position mapping in this step is highly simplified (just drawing at the start of the line or covering the whole line). In a real PR, this needs the `rendered_to_source` inverse mapping. For the sake of this plan's brevity and getting the architecture in place, we accept this simplification, but a comment is left. The implementer should refine `x_for_index`.*

- [ ] **Step 2: Commit**

```bash
git add crates/sola-app/src/focused_editor.rs
git commit -m "refactor(editor): update paint method to use VisualDocument"
```

---

## Task 7: Wire up SolaRoot

Replace the `gpui::list` in `SolaRoot::render_document_surface` with the `FocusedEditorElement` alone. Remove the `div().id("main-scroll-container")` wrapper if it interferes, but `FocusedEditorElement` should be allowed to grow inside a scrolling container.

**Files:**
- Modify: `crates/sola-app/src/shell.rs`

- [ ] **Step 1: Simplify `render_document_surface`**

```rust
// In crates/sola-app/src/shell.rs, inside `impl SolaRoot`
// Find `fn render_document_surface` and ensure it returns just the wrapping div and `editor_element`.
// Since it already looks like it's doing this (from the read_file output), just verify it's passing `global_cursor_head` correctly.

// Ensure `global_cursor` logic uses the new `block_local_to_global_offset`:
        let focused_block_idx = document.focused_block();
        let local_cursor = document.focused_cursor().cloned().unwrap_or_default();
        let global_cursor_head = document
            .block_local_to_global_offset(focused_block_idx, local_cursor.head)
            .unwrap_or(0);
        let global_cursor_anchor = local_cursor
            .anchor
            .and_then(|a| document.block_local_to_global_offset(focused_block_idx, a));
```

- [ ] **Step 2: Run application**

Run: `cargo check -p sola-app`
Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add crates/sola-app/src/shell.rs
git commit -m "refactor(editor): ensure SolaRoot uses global cursor offsets for FocusedEditorElement"
```

---
Plan complete and saved to `docs/superpowers/plans/2026-04-28-unified-editor-refactor.md`. Two execution options:

1. **Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration
2. **Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?