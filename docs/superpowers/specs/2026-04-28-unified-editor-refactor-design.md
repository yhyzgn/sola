# Design Spec: Unified Editor Refactor (Phase 2)

## 1. Goal
Refactor Sola's editor from a block-based "Div Soup" (GPUI list of divs) to a unified, high-performance canvas-based editor (`FocusedEditorElement`). This shift enables seamless cross-block selection, pixel-perfect layout, and provides a foundation for high-fidelity exports (PDF, Image, HTML).

## 2. Architecture: The Unified Canvas

### 2.1 Component: `FocusedEditorElement`
A custom `gpui::Element` that takes the entire `DocumentModel` and renders it on a single canvas.

- **Responsibilities**:
    - Flatten AST blocks into a continuous sequence of `TextRun`s.
    - Manage scrolling and viewport virtualization (only layout/paint what's visible).
    - Handle global hit-testing and coordinate mapping (Pixel -> Global UTF-8 Offset).
    - Coordinate the painting of text, selections, carets, and inline objects.

### 2.2 Global Offset Model
All state (Cursor, Selection) moves from "Block-Local" to "Document-Global" byte offsets.
- **Selection**: A simple `Range<usize>` in the document's global UTF-8 byte stream.
- **Caret**: A single `usize` offset.
- **Consistency**: This eliminates selection "jumps" at block boundaries and simplifies cross-block dragging.

## 3. Rendering Logic: Dual-State "Live Preview"

The mapping from AST to `TextRun`s is dynamic and per-block:

### 3.1 Focused Block (Source Mode)
- **Condition**: If `global_cursor` intersects the block's range.
- **Output**: 
    - Full Markdown source text.
    - Syntax highlighting via Tree-sitter (emitted as `TextRun`s).
    - Monospace font (e.g., JetBrains Mono).

### 3.2 Blurred Block (Rich Mode)
- **Condition**: If `global_cursor` is outside the block.
- **Output**:
    - **Marker Hiding**: Markdown symbols (`#`, `*`, `_`, `[ ]`) are omitted or converted to zero-width runs.
    - **Styling**: Runs use proportional fonts (System UI/Serif). Headers use larger font sizes and bold weights.
    - **Inline Replacement**: Math (`$...$`) is replaced by a single `U+FFFC` (Object Replacement Character).

## 4. Inline Objects & Layout Engine

### 4.1 Object Replacement (U+FFFC)
- Inline math and images are not "text". They are objects.
- During layout, we insert a placeholder character into the `TextSystem`.
- We use the layout results to find the exact `(x, y)` of that placeholder.
- During the paint phase, we overlay the SVG (rendered via Typst) at those coordinates.

### 4.2 Decoupled Layout Engine (`SolaLayoutEngine`)
To support the "What You See Is What You Get" (WYSIWYG) export requirement:
- **Layout Logic**: Abstracted into a pure function/struct that returns a `VisualDocument` (list of visual lines and object positions).
- **Target: Editor**: GPUI implementation consumes `VisualDocument` to draw to screen.
- **Target: Export**: 
    - **Image/PDF**: Uses `tiny-skia` or `Typst` to draw the same `VisualDocument` to a buffer.
    - **HTML**: Converts the flattened runs into a semantic HTML structure with embedded SVG assets.

## 5. Implementation Strategy
1.  **Phase 1**: Update `DocumentModel` to provide a global-to-local offset mapping utility.
2.  **Phase 2**: Implement the `TextRun` generator in `focused_editor.rs` that handles the Focused/Blurred logic.
3.  **Phase 3**: Replace the `gpui::list` in `shell.rs` with the new `FocusedEditorElement`.
4.  **Phase 4**: Port interaction logic (Click, Drag, Arrows) to the new Global Offset model.

## 6. Success Criteria
- [ ] Cursor can move from the first character of the document to the last character without ever "losing" focus or jumping.
- [ ] Selecting text across 3+ blocks (including headers and code fences) works flawlessly.
- [ ] Inline math renders correctly inside blurred paragraphs and stays consistent during scrolling.
- [ ] The architecture explicitly defines hooks for future PDF/Image export.
