# Inline Math And Cursor Hit Testing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add inline math rendering for paragraph-like blocks, then add mouse click cursor positioning inside the focused editor surface.

**Architecture:** Reuse the existing block-level `TypstAdapter` pipeline for paragraph/list/quote blocks that contain inline `$...$`, rendering those blurred blocks through Typst as whole-line previews. For cursor hit testing, keep the current focused-block renderer but add click handlers plus lightweight coordinate-to-offset mapping so the cursor can move on mouse clicks without rewriting the editor surface.

**Tech Stack:** Rust, GPUI, Typst, existing `sola-document` / `sola-app` workspace crates

---

### Task 1: Inline Math Rendering For Paragraph-Like Blocks

**Files:**
- Modify: `crates/sola-document/src/lib.rs`
- Modify: `crates/sola-app/src/shell.rs`
- Modify: `crates/sola-core/src/lib.rs`

- [x] Add failing tests in `sola-document` for inline `$...$` detection on paragraph-like blocks and for rebuilding `Pending` render state after metadata refresh.
- [x] Run `cargo test -p sola-document inline_math` (or the exact new test names) and verify the new tests fail for the expected reason.
- [x] Extend `sola-document` so paragraph/list/quote blocks with inline `$...$` get `TypstAdapter::Pending`, while plain text blocks continue using the native text path.
- [x] Add or extend `sola-app` tests so `typst_render_request` maps inline-math paragraph-like blocks to `RenderKind::Block`.
- [x] Run `cargo test -p sola-document` and `cargo test -p sola-app` to verify the inline math path is green.

### Task 2: Mouse Click Cursor Positioning

**Files:**
- Modify: `crates/sola-document/src/lib.rs`
- Modify: `crates/sola-app/src/shell.rs`

- [x] Add failing tests in `sola-document` for setting cursor offsets directly and clamping them to text bounds.
- [x] Add failing `sola-app` tests for the coordinate-to-offset helper that mouse clicks will use.
- [x] Run the focused test commands and confirm the red step is real.
- [x] Implement document-level cursor setters plus app-side click-to-offset mapping and wire `on_mouse_down(MouseButton::Left, ...)` into the focused block renderer.
- [x] Run `cargo test -p sola-document`, `cargo test -p sola-app`, and then `cargo test --workspace` to verify the feature set is green.

### Task 3: Final Verification And Handover

**Files:**
- Modify: `.codex/04-current-status.md`
- Modify: `.codex/worklog.md`

- [x] Run `cargo fmt --all`.
- [x] Run `cargo test --workspace`.
- [x] Run `cargo run` and confirm the current environment behavior is still clean.
- [x] Update `.codex` status and worklog with the inline math and mouse cursor milestones.
