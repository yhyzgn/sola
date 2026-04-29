use gpui::{size, Bounds, Pixels, Point, SharedString, Window, WrappedLine, px};
use crate::focused_editor::EditorBlock;

/// Represents an inline object (like math or an image) that needs space reserved.
#[derive(Debug, Clone)]
pub struct VisualObject {
    pub global_offset: usize,
    pub width: Pixels,
    pub height: Pixels,
    pub kind: crate::focused_editor::EditorObjectKind,
}

/// A single visually wrapped line of text, plus any objects that appear on this line.
#[derive(Clone)]
pub struct VisualLine {
    pub block_index: usize,
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

    for (block_index, block) in blocks.iter().enumerate() {
        let block_wrap_width = wrap_width - block.indentation;
        let lines = window
            .text_system()
            .shape_text(
                SharedString::from(block.text.clone()),
                block.font_size,
                &block.runs,
                Some(block_wrap_width),
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
                    origin: Point { x: block.indentation, y: current_y },
                    size: size(block_wrap_width, line_height),
                };

                // Find objects in this visual line
                let mut objects = Vec::new();
                for obj in &block.objects {
                    let obj_rendered_start = block.source_to_rendered(obj.start);
                    // Check if the placeholder U+FFFC is in this line
                    if obj_rendered_start >= text_start && obj_rendered_start < text_end {
                        let x = line.unwrapped_layout.x_for_index(obj_rendered_start - block_rendered_base);
                        
                        let (width, height) = match &obj.kind {
                            crate::focused_editor::EditorObjectKind::Math { .. } => (px(40.0), line_height - px(4.0)),
                            crate::focused_editor::EditorObjectKind::Checkbox { .. } => (px(18.0), line_height),
                        };

                        objects.push((
                            VisualObject {
                                global_offset: block.global_start + obj.start,
                                width,
                                height,
                                kind: obj.kind.clone(),
                            },
                            Point { x, y: px(0.0) } // Relative to line bounds
                        ));
                    }
                }

                doc.lines.push(VisualLine {
                    block_index,
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
