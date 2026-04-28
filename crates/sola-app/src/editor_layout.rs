use gpui::{Bounds, Pixels, Point, TextRun, Window, WrappedLine};

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
    pub bounds: Bounds<Pixels>,
    pub wrapped_line: WrappedLine,
    pub objects: Vec<(VisualObject, Point<Pixels>)>, // Object and its relative (x, y) offset within the line
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
