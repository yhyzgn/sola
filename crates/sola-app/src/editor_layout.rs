use gpui::{size, Bounds, Pixels, Point, SharedString, Window, WrappedLine, px};
use crate::focused_editor::EditorBlock;

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

pub fn layout_document(
    window: &mut Window,
    blocks: &[EditorBlock],
    wrap_width: Pixels,
) -> VisualDocument {
    let mut doc = VisualDocument::new();
    let mut current_y = Pixels::ZERO;

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
            boundaries.push(line.text.len());

            let mut local_start = 0;
            for local_end in boundaries {
                let text_start = block_rendered_base + local_start;
                let text_end = block_rendered_base + local_end;

                let global_start = block.global_start + block.rendered_to_source(text_start);
                let global_end = block.global_start + block.rendered_to_source(text_end);

                let bounds = Bounds {
                    origin: Point { x: Pixels::ZERO, y: current_y },
                    size: size(wrap_width, line_height),
                };

                // Find objects in this visual line
                let mut objects = Vec::new();
                for deco in &block.inline_math {
                    let deco_rendered_start = block.source_to_rendered(deco.start);
                    // Check if the placeholder U+FFFC is in this line
                    if deco_rendered_start >= text_start && deco_rendered_start < text_end {
                        let x = line.unwrapped_layout.x_for_index(deco_rendered_start - block_rendered_base);
                        
                        objects.push((
                            VisualObject {
                                global_offset: block.global_start + deco.start,
                                width: px(40.0), // Placeholder width for now
                                height: line_height - px(4.0),
                                cache_key: deco.cache_key.clone(),
                            },
                            Point { x, y: px(2.0) } // Relative to line bounds
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
