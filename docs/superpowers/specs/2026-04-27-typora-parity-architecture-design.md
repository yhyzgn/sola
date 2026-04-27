# Sola Editor: Typora Parity Re-architecture Design

## 1. Goal
Refactor Sola's main text editing area from a "Div Soup" (stacking individual GPUI `Div` elements per AST block) into a unified, high-performance `FocusedEditorElement` that utilizes GPUI's low-level `TextSystem`. The goal is to achieve pixel-perfect hit-testing, smooth cross-block text selection, and a true "Live Preview" (WYSIWYG) experience identical to Typora, where the currently focused block displays Markdown source while unfocused blocks render as fully formatted rich text.

## 2. Architecture: Unified Text Buffer with Dynamic Layout

### 2.1 The Core Element (`FocusedEditorElement`)
- We will discard the approach of rendering a `gpui::list` of separate `Div` blocks.
- Instead, the entire document (or a virtualized window of it) will be rendered inside a single custom `gpui::Element` named `FocusedEditorElement`.
- This element owns the responsibility of mapping the abstract `DocumentModel` (AST) into visual `TextRun`s and drawing them on the screen.

### 2.2 Global Offset vs. Block Offset
- **Model Layer**: The `DocumentModel` still maintains the block structure (Paragraph, CodeFence, Math, etc.) because this is necessary for incremental parsing and structure manipulation.
- **View Layer**: The `FocusedEditorElement` flattens these blocks into a single continuous stream of `TextRun`s.
- **Cursor/Selection**: The `CursorState` will operate on a **Global UTF-8 Byte Offset** relative to the entire document (or the currently rendered chunk), rather than a local offset within a specific block. This is the key to enabling seamless cross-block dragging and selection.

### 2.3 Dual-State Layout Engine (Live Preview)
The mapping from AST to `TextRun`s is dynamic and depends on the cursor position:

1.  **Focused Block (Source Mode)**:
    - If the global cursor intersects a block's span, that block is rendered in "Source Mode".
    - Markdown syntax characters (`#`, `*`, `_`, `$`) are visible.
    - Rendered using the monospace font (e.g., JetBrains Mono) with syntax highlighting via Tree-sitter.

2.  **Blurred Block (Rich Text Mode)**:
    - If the cursor is outside the block, it is rendered in "Rich Text Mode".
    - Markdown syntax characters are hidden (or mapped to zero-width runs).
    - **Typography**: Text uses proportional fonts (e.g., System UI font or a Serif font for body). Headers use larger font sizes and bolder weights.
    - **Inline Elements**: Links are styled differently, and inline math (`$...$`) is replaced by an inline SVG image (using GPUI's `Image` run capability within `TextSystem`, or by carefully positioning absolute image quads over reserved whitespace).

### 2.4 Hit-Testing and Caret
- By relying entirely on `TextSystem::shape_text` and `WrappedLine`, we inherit GPUI's robust logic for translating between `(x, y)` pixel coordinates and text index offsets.
- The caret will be drawn exactly where `WrappedLine::x_for_index` dictates, regardless of whether it's in a focused or blurred block.

## 3. Data Flow

1.  `DocumentModel` changes -> `Workspace` emits event -> `SolaRoot` triggers update.
2.  `FocusedEditorElement` receives the new `DocumentModel` and the current `CursorState` (global offset).
3.  **Layout Phase**:
    - `FocusedEditorElement` iterates through the AST blocks.
    - For each block, it checks if `cursor.head` is within the block's range.
    - It generates a list of `TextRun`s based on the focused/blurred state.
    - It calls `window.text_system().shape_text` with the combined `TextRun`s to get `WrappedLine`s.
4.  **Paint Phase**:
    - Draw Selection Backgrounds (translating global index ranges to visual rectangles using `WrappedLine` boundaries).
    - Draw Text (using `WrappedLine::paint`).
    - Draw Inline Images (Math/Typst) at calculated offsets.
    - Draw Caret.

## 4. Complexity and De-risking
- **Challenge**: Inline Images (Typst Math). GPUI's `TextSystem` primarily deals with text.
- **Solution**: We will map inline math to a "replacement character" (e.g., `U+FFFC` Object Replacement Character) with a custom font or explicitly calculated whitespace run to reserve horizontal space in the `WrappedLine`. During the paint phase, we overlay the rendered SVG at the exact coordinates of that replacement character.

## 5. Scope
This design outlines the fundamental rendering shift. It replaces the current `gpui::list` + `div()` approach in `shell.rs` with the `FocusedEditorElement` approach started in `focused_editor.rs`, but significantly expanding it to handle the whole document and dual-state rich text formatting.