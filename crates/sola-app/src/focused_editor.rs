use gpui::{
    App, Bounds, Element, ElementId, Font, FontFeatures, FontStyle, FontWeight,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Point, SharedString,
    Style, TextRun, Window, WrappedLine, px,
};
use sola_document::{BlockKind, CursorState, DocumentModel, TypstAdapter};
use sola_document::highlighter::{HighlightKind, HighlightedSpan, SyntaxHighlighter};
use sola_theme::{Theme, parse_hex_color};
use crate::editor_layout::{VisualDocument, layout_document};
use std::sync::Arc;
use std::collections::HashMap;

fn rgb_hex(hex: &str) -> Hsla {
    gpui::rgb(parse_hex_color(hex).unwrap_or(0xffffff)).into()
}

pub fn spans_to_runs(
    spans: &[HighlightedSpan],
    style: &FocusedEditorStyle,
    theme: &Theme,
) -> Vec<TextRun> {
    let syntax = &theme.syntax;
    let palette = &theme.palette;

    spans
        .iter()
        .map(|span| {
            let color = match span.kind {
                HighlightKind::Keyword => &syntax.keyword,
                HighlightKind::String => &syntax.string,
                HighlightKind::Comment => &syntax.comment,
                HighlightKind::Function => &syntax.function,
                HighlightKind::Number => &syntax.number,
                HighlightKind::Constant => &syntax.constant,
                HighlightKind::TypeName => &syntax.type_name,
                HighlightKind::Other => &palette.text_primary,
            };

            TextRun {
                len: span.text.len(),
                font: Font {
                    family: style.font_family.into(),
                    features: gpui::FontFeatures::default(),
                    fallbacks: None,
                    weight: FontWeight::default(),
                    style: FontStyle::default(),
                },
                color: rgb_hex(color),
                background_color: None,
                underline: None,
                strikethrough: None,
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct FocusedEditorStyle {
    pub font_family: &'static str,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub padding_x: Pixels,
    pub padding_y: Pixels,
    pub caret_width: Pixels,
}

impl FocusedEditorStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        let font_size = px(theme.typography.code_size as f32);
        let line_height = px(theme.typography.code_size as f32 * 1.35);

        Self {
            font_family: "JetBrains Mono",
            font_size,
            line_height,
            padding_x: px(40.0),
            padding_y: px(20.0),
            caret_width: px(2.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrappedVisualLine {
    pub start: usize,
    pub end: usize,
    pub row: usize,
}

#[derive(Clone)]
pub(crate) struct VisualLineRef {
    global_start: usize,
    global_end: usize,
    rendered_local_start: usize,
    wrapped_line_start: usize,
    local_row: usize,
    global_row: usize,
    y_offset: Pixels,
    line_height: Pixels,
    block_index: usize,
    line: WrappedLine,
}

pub fn shape_focused_lines(
    window: &mut Window,
    text: &str,
    style: &FocusedEditorStyle,
    color: Hsla,
    wrap_width: Pixels,
) -> Option<Vec<WrappedLine>> {
    let run = TextRun {
        len: text.len(),
        font: Font {
            family: style.font_family.into(),
            features: FontFeatures::default(),
            fallbacks: None,
            weight: FontWeight::default(),
            style: FontStyle::default(),
        },
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };

    window
        .text_system()
        .shape_text(
            SharedString::from(text.to_string()),
            style.font_size,
            &[run],
            Some(wrap_width),
            None,
        )
        .ok()
        .map(|lines| lines.into_vec())
}

pub fn approximate_editor_wrap_width(available_width: Pixels) -> Pixels {
    // We target a 900px centered container with 40px padding on each side.
    // So the actual text wrap width should never exceed 900 - 80 = 820px.
    let max_text_width = px(820.0);
    let width = available_width - px(80.0); // Account for padding
    
    if width > max_text_width {
        max_text_width
    } else if width > px(120.0) {
        width
    } else {
        px(120.0)
    }
}

pub fn move_cursor_vertical_visual(
    visual_lines: &[VisualLineRef],
    blocks: &[EditorBlock],
    current_offset: usize,
    delta: isize,
) -> Option<usize> {
    let current_visual_line_idx = find_visual_line_ref(visual_lines, current_offset)?;
    let target_global_row = shift_visual_row(
        visual_lines.get(current_visual_line_idx)?.global_row,
        delta,
        visual_lines.len(),
    )?;
    let target_visual_line_idx = visual_lines
        .iter()
        .position(|line| line.global_row == target_global_row)?;
    let current = &visual_lines[current_visual_line_idx];
    let target = &visual_lines[target_visual_line_idx];

    let current_block = &blocks[current.block_index];
    let target_block = &blocks[target.block_index];

    let current_rendered_offset =
        current_block.source_to_rendered(current_offset - current_block.global_start);
    let current_local = current_rendered_offset.saturating_sub(current.rendered_local_start);

    let current_point = current.line.position_for_index(
        current.wrapped_line_start + current_local,
        current.line_height,
    )?;

    let target_local_rendered = target
        .line
        .closest_index_for_position(
            Point {
                x: current_point.x,
                y: target.line_height * target.local_row as f32,
            },
            target.line_height,
        )
        .unwrap_or_else(|index| index);

    let target_rendered_offset = target.rendered_local_start
        + target_local_rendered.saturating_sub(target.wrapped_line_start);

    Some(target_block.global_start + target_block.rendered_to_source(target_rendered_offset))
}

pub fn visual_line_edge_offset(
    lines: &[WrappedVisualLine],
    current_offset: usize,
    line_end: bool,
) -> Option<usize> {
    let current_visual_line = find_visual_line(lines, current_offset)?;
    let line = lines.get(current_visual_line)?;

    Some(if line_end { line.end } else { line.start })
}

pub(crate) fn collect_visual_lines(
    lines: &[WrappedLine],
    line_height: Pixels,
    block_index: usize,
    global_start_base: usize,
) -> Vec<VisualLineRef> {
    let mut visual = Vec::new();
    let mut global_base = global_start_base;
    let mut global_row = 0;
    let mut current_y = Pixels::ZERO;

    for line in lines {
        let mut boundaries = line
            .wrap_boundaries()
            .iter()
            .map(|boundary| {
                let run = &line.runs()[boundary.run_ix];
                run.glyphs[boundary.glyph_ix].index
            })
            .collect::<Vec<_>>();
        boundaries.push(line.len());

        let mut local_start = 0;
        for (local_row, local_end) in boundaries.into_iter().enumerate() {
            visual.push(VisualLineRef {
                global_start: global_base + local_start,
                global_end: global_base + local_end,
                rendered_local_start: local_start,
                wrapped_line_start: local_start,
                local_row,
                global_row,
                y_offset: current_y,
                line_height,
                block_index,
                line: line.clone(),
            });
            local_start = local_end;
            global_row += 1;
            current_y += line_height;
        }

        global_base += line.text.len() + 1;
    }

    visual
}

pub fn visual_line_ranges(lines: &[WrappedLine], line_height: Pixels) -> Vec<WrappedVisualLine> {
    collect_visual_lines(lines, line_height, 0, 0)
        .into_iter()
        .map(|line| WrappedVisualLine {
            start: line.global_start,
            end: line.global_end,
            row: line.global_row,
        })
        .collect()
}

fn find_visual_line(lines: &[WrappedVisualLine], offset: usize) -> Option<usize> {
    lines
        .iter()
        .position(|line| offset >= line.start && offset <= line.end)
}

fn find_visual_line_ref(lines: &[VisualLineRef], offset: usize) -> Option<usize> {
    lines
        .iter()
        .position(|line| offset >= line.global_start && offset <= line.global_end)
}

fn shift_visual_row(current_row: usize, delta: isize, total_rows: usize) -> Option<usize> {
    let target = current_row.checked_add_signed(delta)?;
    (target < total_rows).then_some(target)
}

pub struct FocusedEditorElement {
    blocks: Vec<EditorBlock>,
    typst_cache: HashMap<String, TypstAdapter>,
    style: FocusedEditorStyle,
    cursor: Option<CursorState>,
    cursor_visible: bool,
    selection_color: Hsla,
    cursor_color: Hsla,
    on_cursor_move: Option<Arc<dyn Fn(usize, bool, &mut Window, &mut App) + Send + Sync>>,
}

impl FocusedEditorElement {
    pub fn new(
        blocks: Vec<EditorBlock>,
        typst_cache: HashMap<String, TypstAdapter>,
        style: FocusedEditorStyle,
        cursor: Option<CursorState>,
        cursor_visible: bool,
        selection_color: Hsla,
        cursor_color: Hsla,
    ) -> Self {
        Self {
            blocks,
            typst_cache,
            style,
            cursor,
            cursor_visible,
            selection_color,
            cursor_color,
            on_cursor_move: None,
        }
    }

    pub fn on_cursor_move(
        mut self,
        callback: impl Fn(usize, bool, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_cursor_move = Some(Arc::new(callback));
        self
    }
}

#[derive(Clone, Debug)]
pub struct InlineDecoration {
    pub start: usize,
    pub end: usize,
    pub cache_key: String,
}

#[derive(Clone)]
pub struct EditorBlock {
    pub text: String,
    pub runs: Vec<TextRun>,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub global_start: usize,
    pub is_focused: bool,
    pub kind: BlockKind,
    pub inline_math: Vec<InlineDecoration>,
}

impl EditorBlock {
    pub fn source_to_rendered(&self, source_local: usize) -> usize {
        if self.is_focused || self.inline_math.is_empty() {
            return source_local;
        }

        let mut rendered_offset = 0;
        let mut last_source_pos = 0;

        for deco in &self.inline_math {
            if source_local < deco.start {
                rendered_offset += source_local - last_source_pos;
                return rendered_offset;
            }
            rendered_offset += deco.start - last_source_pos;
            if source_local < deco.end {
                // Inside a formula, clamp to the placeholder position
                return rendered_offset;
            }
            rendered_offset += 1; // The placeholder \u{FFFC}
            last_source_pos = deco.end;
        }

        rendered_offset + (source_local - last_source_pos)
    }

    pub fn rendered_to_source(&self, rendered_local: usize) -> usize {
        if self.is_focused || self.inline_math.is_empty() {
            return rendered_local;
        }

        let mut current_rendered = 0;
        let mut current_source = 0;

        for deco in &self.inline_math {
            let gap = deco.start - current_source;
            if rendered_local < current_rendered + gap {
                return current_source + (rendered_local - current_rendered);
            }
            current_rendered += gap;

            if rendered_local == current_rendered {
                return deco.start; // Start of placeholder maps to start of formula
            }
            current_rendered += 1;
            current_source = deco.end;
        }

        current_source + (rendered_local - current_rendered)
    }
}

pub fn generate_editor_blocks(
    doc: &DocumentModel,
    global_cursor: Option<usize>,
    style: &FocusedEditorStyle,
    theme: &Theme,
) -> Vec<EditorBlock> {
    let mut blocks = Vec::new();
    let focused_block_idx =
        global_cursor.and_then(|c| doc.global_offset_to_block_local(c).map(|(idx, _)| idx));
    let mut current_global = 0;

    for (i, block) in doc.blocks().iter().enumerate() {
        let is_focused = focused_block_idx == Some(i);

        let (text, font_size, line_height, runs, inline_math) = if is_focused {
            let highlighter = SyntaxHighlighter::new_rust();
            let spans = highlighter.highlight(&block.source);
            let runs = spans_to_runs(&spans, style, theme);
            (
                block.source.clone(),
                style.font_size,
                style.line_height,
                runs,
                Vec::new(),
            )
        } else {
            let mut text = String::new();
            let mut inline_math = Vec::new();
            let mut last_pos = 0;

            // Simple scanner for $...$
            let bytes = block.source.as_bytes();
            let mut pos = 0;
            while pos < bytes.len() {
                if bytes[pos] == b'$' && (pos == 0 || bytes[pos - 1] != b'\\') {
                    if let Some(end_rel) = block.source[pos + 1..].find('$') {
                        let end = pos + 1 + end_rel;
                        let formula = &block.source[pos + 1..end];
                        if !formula.contains('\n') && !formula.is_empty() {
                            // Found valid inline math
                            text.push_str(&block.source[last_pos..pos]);
                            text.push('\u{FFFC}'); // Placeholder
                            
                            inline_math.push(InlineDecoration {
                                start: pos,
                                end: end + 1,
                                cache_key: format!("math::{}", formula),
                            });
                            
                            pos = end + 1;
                            last_pos = pos;
                            continue;
                        }
                    }
                }
                pos += 1;
            }
            text.push_str(&block.source[last_pos..]);

            // Styling
            let (size_mult, weight, color) = match &block.kind {
                BlockKind::Heading { level: 1 } => {
                    (2.0, FontWeight::BOLD, &theme.palette.text_primary)
                }
                BlockKind::Heading { level: 2 } => {
                    (1.5, FontWeight::BOLD, &theme.palette.text_primary)
                }
                BlockKind::Heading { level: 3 } => {
                    (1.25, FontWeight::BOLD, &theme.palette.text_primary)
                }
                BlockKind::Heading { .. } => (1.1, FontWeight::BOLD, &theme.palette.text_primary),
                BlockKind::Quote => (1.0, FontWeight::NORMAL, &theme.palette.text_muted),
                _ => (1.0, FontWeight::NORMAL, &theme.palette.text_primary),
            };

            let base_size = theme.typography.body_size as f32;
            let font_size = px(base_size * size_mult);
            let line_height = font_size * 1.5;

            let runs = vec![TextRun {
                len: text.len(),
                font: Font {
                    family: "System UI".into(),
                    features: FontFeatures::default(),
                    fallbacks: None,
                    weight,
                    style: FontStyle::default(),
                },
                color: rgb_hex(color),
                background_color: None,
                underline: None,
                strikethrough: None,
            }];
            (text, font_size, line_height, runs, inline_math)
        };

        let source_len = block.source.len();
        blocks.push(EditorBlock {
            text,
            runs,
            font_size,
            line_height,
            global_start: current_global,
            is_focused,
            kind: block.kind.clone(),
            inline_math,
        });

        current_global += source_len + 2;
    }

    blocks
}

pub struct FocusedEditorState {
    pub(crate) visual_doc: VisualDocument,
}

impl Element for FocusedEditorElement {
    type RequestLayoutState = FocusedEditorState;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let wrap_width = approximate_editor_wrap_width(window.bounds().size.width);

        let visual_doc = layout_document(window, &self.blocks, wrap_width);

        // Calculate total height to enable scrolling
        let total_height = visual_doc.total_height + px(100.0); // Add bottom padding

        let mut style = Style::default();
        style.size.width = gpui::relative(1.0).into();
        style.size.height = total_height.into();

        let layout_id = window.request_layout(style, None, cx);
        (
            layout_id,
            FocusedEditorState {
                visual_doc,
            },
        )
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout_state: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        ()
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout_state: &mut Self::RequestLayoutState,
        _prepaint_state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let visual_doc = &request_layout_state.visual_doc;

        let padding = Point {
            x: self.style.padding_x,
            y: self.style.padding_y,
        };
        let text_bounds = Bounds {
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
                        let rect = Bounds {
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
                    let svg_bounds = Bounds {
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
                        let caret_bounds = Bounds {
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
}

impl IntoElement for FocusedEditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sola_theme::Theme;

    #[test]
    fn editor_style_derives_compact_code_metrics_from_theme() {
        let style = FocusedEditorStyle::from_theme(&Theme::sola_dark());

        assert_eq!(style.font_family, "JetBrains Mono");
        assert_eq!(style.font_size, px(14.0));
        assert_eq!(style.line_height, px(18.9));
        assert_eq!(style.padding_x, px(40.0));
        assert_eq!(style.padding_y, px(20.0));
        assert_eq!(style.caret_width, px(2.0));
    }

    #[test]
    fn approximate_wrap_width_reserves_sidebar_and_padding_budget() {
        assert_eq!(approximate_editor_wrap_width(px(1000.0)), px(820.0));
        assert_eq!(approximate_editor_wrap_width(px(300.0)), px(220.0));
    }

    #[test]
    fn shift_visual_row_respects_bounds() {
        assert_eq!(shift_visual_row(1, 1, 3), Some(2));
        assert_eq!(shift_visual_row(1, -1, 3), Some(0));
        assert_eq!(shift_visual_row(0, -1, 3), None);
        assert_eq!(shift_visual_row(2, 1, 3), None);
    }

    #[test]
    fn visual_line_edge_offset_returns_current_visual_line_edges() {
        let lines = vec![
            WrappedVisualLine {
                start: 0,
                end: 10,
                row: 0,
            },
            WrappedVisualLine {
                start: 11,
                end: 14,
                row: 1,
            },
        ];

        assert_eq!(visual_line_edge_offset(&lines, 3, false), Some(0));
        assert_eq!(visual_line_edge_offset(&lines, 3, true), Some(10));
        assert_eq!(visual_line_edge_offset(&lines, 11, false), Some(11));
        assert_eq!(visual_line_edge_offset(&lines, 11, true), Some(14));
    }

    #[test]
    fn test_editor_block_generation() {
        let doc = DocumentModel::from_markdown("# H1\n\nText");
        let theme = Theme::sola_dark();
        let style = FocusedEditorStyle::from_theme(&theme);

        // Cursor at 0 (inside H1), H1 is Source, Text is Rich
        let blocks = generate_editor_blocks(&doc, Some(0), &style, &theme);

        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].is_focused);
        assert!(!blocks[1].is_focused);
        
        // Block 0: "# H1" (source)
        assert_eq!(blocks[0].text, "# H1");
        // Block 1: "Text" (rendered)
        assert_eq!(blocks[1].text, "Text");
    }

    #[test]
    fn test_editor_block_offset_mapping() {
        // Focused block (1:1 mapping)
        let focused = EditorBlock {
            text: "# H1".into(),
            runs: vec![],
            font_size: gpui::px(14.0),
            line_height: gpui::px(20.0),
            global_start: 0,
            source_len: 4,
            is_focused: true,
            kind: BlockKind::Heading { level: 1 },
            inline_math: vec![],
        };
        assert_eq!(focused.rendered_to_source(2), 2);
        assert_eq!(focused.source_to_rendered(2), 2);

        // Blurred block (with inline math mapping)
        let blurred = EditorBlock {
            text: "Hello \u{FFFC} world".into(),
            runs: vec![],
            font_size: gpui::px(14.0),
            line_height: gpui::px(20.0),
            global_start: 0,
            source_len: 20,
            is_focused: false,
            kind: BlockKind::Paragraph,
            inline_math: vec![InlineDecoration {
                start: 6,
                end: 14,
                cache_key: "math::e=mc^2".into(),
            }],
        };
        // "Hello " (len 6) maps 1:1
        assert_eq!(blurred.rendered_to_source(0), 0);
        assert_eq!(blurred.rendered_to_source(5), 5);
        // Placeholder at index 6 maps to start of math (6)
        assert_eq!(blurred.rendered_to_source(6), 6);
        // After placeholder (index 7) maps to after math (14)
        assert_eq!(blurred.rendered_to_source(7), 14);
        
        // Source to Rendered
        assert_eq!(blurred.source_to_rendered(0), 0);
        assert_eq!(blurred.source_to_rendered(6), 6);
        assert_eq!(blurred.source_to_rendered(10), 6); // inside math clamps to placeholder
        assert_eq!(blurred.source_to_rendered(14), 7); // after math
    }

    #[test]
    fn test_generate_editor_blocks_replaces_inline_math() {
        let source = "Hello $e=mc^2$ world";
        let doc = DocumentModel::from_markdown(source);
        let theme = Theme::sola_dark();
        let style = FocusedEditorStyle::from_theme(&theme);
        
        // global_cursor = None means no block is focused
        let blocks = generate_editor_blocks(&doc, None, &style, &theme);
        
        assert_eq!(blocks.len(), 1);
        let block = &blocks[0];
        
        // Expected text: "Hello \u{FFFC} world"
        assert_eq!(block.text, "Hello \u{FFFC} world");
        assert_eq!(block.inline_math.len(), 1);
        assert_eq!(block.inline_math[0].start, 6); // "$" at index 6
        assert_eq!(block.inline_math[0].end, 14); // after second "$"
    }
}
