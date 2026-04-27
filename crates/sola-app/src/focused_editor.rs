use gpui::{
    App, Bounds, DispatchPhase, Element, ElementId, Font, FontFeatures, FontStyle, FontWeight,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, Pixels, Point, SharedString, Style, TextAlign, TextRun, Window, WrappedLine,
    px,
};
use sola_document::{BlockKind, CursorState, DocumentModel};
use sola_document::highlighter::{HighlightKind, HighlightedSpan, SyntaxHighlighter};
use sola_theme::{Theme, parse_hex_color};
use std::sync::Arc;

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
            padding_x: px(6.0),
            padding_y: px(6.0),
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

pub fn approximate_editor_wrap_width(window_width: Pixels) -> Pixels {
    let width = window_width - px(420.0);
    if width > px(120.0) { width } else { px(120.0) }
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

pub fn hit_test_visual_offset(
    visual_lines: &[VisualLineRef],
    blocks: &[EditorBlock],
    point: Point<Pixels>,
) -> Option<usize> {
    let line = visual_lines
        .iter()
        .find(|l| point.y >= l.y_offset && point.y < l.y_offset + l.line_height)?;
    let local = line
        .line
        .closest_index_for_position(
            Point {
                x: point.x,
                y: line.line_height * line.local_row as f32,
            },
            line.line_height,
        )
        .unwrap_or_else(|index| index);

    let rendered_offset = line.rendered_local_start + local.saturating_sub(line.wrapped_line_start);
    let block = &blocks[line.block_index];

    Some(block.global_start + block.rendered_to_source(rendered_offset))
}

pub struct FocusedEditorElement {
    blocks: Vec<EditorBlock>,
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
        style: FocusedEditorStyle,
        cursor: Option<CursorState>,
        cursor_visible: bool,
        selection_color: Hsla,
        cursor_color: Hsla,
    ) -> Self {
        Self {
            blocks,
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

#[derive(Clone)]
pub struct EditorBlock {
    pub text: String,
    pub runs: Vec<TextRun>,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub global_start: usize,
    pub source_len: usize,
    pub is_focused: bool,
    pub kind: BlockKind,
}

impl EditorBlock {
    pub fn source_to_rendered(&self, source_local: usize) -> usize {
        if self.is_focused {
            return source_local;
        }
        let prefix_len = self.source_len.saturating_sub(self.text.len());
        source_local.saturating_sub(prefix_len).min(self.text.len())
    }

    pub fn rendered_to_source(&self, rendered_local: usize) -> usize {
        if self.is_focused {
            return rendered_local;
        }
        let prefix_len = self.source_len.saturating_sub(self.text.len());
        (rendered_local + prefix_len).min(self.source_len)
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
    let highlighter = SyntaxHighlighter::new_rust();
    let mut current_global = 0;

    for (i, block) in doc.blocks().iter().enumerate() {
        let is_focused = focused_block_idx == Some(i);
        let source_len = block.source.len();

        let (text, font_size, line_height, runs) = if is_focused {
            let spans = highlighter.highlight(&block.source);
            let runs = spans_to_runs(&spans, style, theme);
            (
                block.source.clone(),
                style.font_size,
                style.line_height,
                runs,
            )
        } else {
            let text = block.rendered.clone();

            // Rich Text Styling based on BlockKind
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
            (text, font_size, line_height, runs)
        };

        blocks.push(EditorBlock {
            text,
            runs,
            font_size,
            line_height,
            global_start: current_global,
            source_len,
            is_focused,
            kind: block.kind.clone(),
        });

        current_global += source_len + 2; // +2 for \n\n
    }

    blocks
}

pub fn layout_editor_blocks(
    window: &mut Window,
    blocks: &[EditorBlock],
    wrap_width: Pixels,
) -> Vec<VisualLineRef> {
    let mut visual_lines = Vec::new();
    let mut current_y = Pixels::ZERO;
    let mut global_row = 0;

    for (block_idx, block) in blocks.iter().enumerate() {
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
            let mut boundaries = line
                .wrap_boundaries()
                .iter()
                .map(|b| line.runs()[b.run_ix].glyphs[b.glyph_ix].index)
                .collect::<Vec<_>>();
            boundaries.push(line.len());

            let mut local_start = 0;
            for (local_row, local_end) in boundaries.into_iter().enumerate() {
                let text_start = block_rendered_base + local_start;
                let text_end = block_rendered_base + local_end;

                visual_lines.push(VisualLineRef {
                    global_start: block.global_start + block.rendered_to_source(text_start),
                    global_end: block.global_start + block.rendered_to_source(text_end),
                    rendered_local_start: text_start,
                    wrapped_line_start: local_start,
                    local_row,
                    global_row,
                    y_offset: current_y,
                    line_height: block.line_height,
                    block_index: block_idx,
                    line: line.clone(),
                });
                local_start = local_end;
                global_row += 1;
                current_y += block.line_height;
            }
            block_rendered_base += line.text.len() + 1;
        }
        // Block spacing
        current_y += block.line_height;
    }

    visual_lines
}

pub struct FocusedEditorState {
    lines: Vec<WrappedLine>,
    visual_lines: Vec<VisualLineRef>,
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
        let style = Style::default();
        let wrap_width = approximate_editor_wrap_width(window.bounds().size.width);

        let visual_lines = layout_editor_blocks(window, &self.blocks, wrap_width);
        let all_lines = visual_lines
            .iter()
            .filter(|l| l.local_row == 0)
            .map(|l| l.line.clone())
            .collect();

        let layout_id = window.request_layout(style, None, cx);
        (
            layout_id,
            FocusedEditorState {
                lines: all_lines,
                visual_lines,
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
        let visual_lines = &request_layout_state.visual_lines;

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
        if let Some(cursor) = &self.cursor
            && let Some(anchor) = cursor.anchor
        {
            let start = anchor.min(cursor.head);
            let end = anchor.max(cursor.head);

            for visual_line in visual_lines {
                let overlap_start = start.max(visual_line.global_start);
                let overlap_end = end.min(visual_line.global_end);

                if overlap_start < overlap_end {
                    let block = &self.blocks[visual_line.block_index];
                    let rendered_start =
                        block.source_to_rendered(overlap_start - block.global_start);
                    let rendered_end = block.source_to_rendered(overlap_end - block.global_start);

                    let local_start = visual_line.wrapped_line_start
                        + (rendered_start - visual_line.rendered_local_start);
                    let local_end = visual_line.wrapped_line_start
                        + (rendered_end - visual_line.rendered_local_start);

                    let x_start = visual_line.line.unwrapped_layout.x_for_index(local_start);
                    let x_end = visual_line.line.unwrapped_layout.x_for_index(local_end);

                    let selection_bounds = Bounds {
                        origin: text_bounds.origin
                            + Point {
                                x: x_start,
                                y: visual_line.y_offset,
                            },
                        size: gpui::size(x_end - x_start, visual_line.line_height),
                    };

                    window.paint_quad(gpui::fill(selection_bounds, self.selection_color));
                }
            }
        }

        // 2. Paint Text
        for visual_line in visual_lines {
            // Only paint the first row of each WrappedLine to avoid over-painting
            if visual_line.local_row == 0 {
                visual_line
                    .line
                    .paint(
                        text_bounds.origin
                            + Point {
                                x: Pixels::ZERO,
                                y: visual_line.y_offset,
                            },
                        visual_line.line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
            }
        }

        // 3. Paint Caret
        if let Some(cursor) = &self.cursor
            && self.cursor_visible
        {
            if let Some(visual_line_idx) = find_visual_line_ref(&visual_lines, cursor.head) {
                let visual_line = &visual_lines[visual_line_idx];
                let block = &self.blocks[visual_line.block_index];
                let rendered_head = block.source_to_rendered(cursor.head - block.global_start);
                let local_offset = visual_line.wrapped_line_start
                    + (rendered_head - visual_line.rendered_local_start);

                let x = visual_line.line.unwrapped_layout.x_for_index(local_offset);

                let caret_bounds = Bounds {
                    origin: text_bounds.origin
                        + Point {
                            x,
                            y: visual_line.y_offset,
                        },
                    size: gpui::size(self.style.caret_width, visual_line.line_height),
                };

                window.paint_quad(gpui::fill(caret_bounds, self.cursor_color));
            }
        }

        // 4. Handle Interactivity (Clicks and Drags)
        if let Some(on_cursor_move) = &self.on_cursor_move {
            let on_cursor_move = on_cursor_move.clone();
            let visual_lines = visual_lines.clone();
            let blocks = self.blocks.clone();

            // Mouse Down: Set anchor and head
            let on_cursor_move_down = on_cursor_move.clone();
            let visual_lines_down = visual_lines.clone();
            let blocks_down = blocks.clone();
            window.on_mouse_event(
                move |event: &MouseDownEvent, phase: DispatchPhase, window, cx| {
                    if phase == DispatchPhase::Bubble && bounds.contains(&event.position) {
                        let local_point = event.position - text_bounds.origin;
                        if let Some(offset) =
                            hit_test_visual_offset(&visual_lines_down, &blocks_down, local_point)
                        {
                            on_cursor_move_down(offset, event.modifiers.shift, window, cx);
                        } else {
                            let end = visual_lines_down.last().map_or(0, |l| l.global_end);
                            on_cursor_move_down(end, event.modifiers.shift, window, cx);
                        }
                    }
                },
            );

            // Mouse Move (Drag): Update head only
            let on_cursor_move_drag = on_cursor_move.clone();
            let visual_lines_drag = visual_lines.clone();
            let blocks_drag = blocks.clone();
            window.on_mouse_event(
                move |event: &MouseMoveEvent, phase: DispatchPhase, window, cx| {
                    if phase == DispatchPhase::Bubble
                        && event.pressed_button == Some(MouseButton::Left)
                    {
                        let local_point = event.position - text_bounds.origin;
                        if let Some(offset) =
                            hit_test_visual_offset(&visual_lines_drag, &blocks_drag, local_point)
                        {
                            on_cursor_move_drag(offset, true, window, cx);
                        } else if local_point.y < Pixels::ZERO {
                            on_cursor_move_drag(0, true, window, cx);
                        } else {
                            let end = visual_lines_drag.last().map_or(0, |l| l.global_end);
                            on_cursor_move_drag(end, true, window, cx);
                        }
                    }
                },
            );
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
        assert_eq!(style.padding_x, px(6.0));
        assert_eq!(style.padding_y, px(6.0));
        assert_eq!(style.caret_width, px(2.0));
    }

    #[test]
    fn approximate_wrap_width_reserves_sidebar_and_padding_budget() {
        assert_eq!(approximate_editor_wrap_width(px(1000.0)), px(580.0));
        assert_eq!(approximate_editor_wrap_width(px(300.0)), px(120.0));
    }

    #[test]
    fn shift_visual_row_respects_bounds() {
        assert_eq!(shift_visual_row(1, 1, 3), Some(2));
        assert_eq!(shift_visual_row(1, -1, 3), Some(0));
        assert_eq!(shift_visual_row(0, -1, 3), None);
        assert_eq!(shift_visual_row(2, 1, 3), None);
    }

    #[test]
    fn visual_row_for_y_respects_bounds() {
        assert_eq!(visual_row_for_y(px(0.0), px(18.0), 3), Some(0));
        assert_eq!(visual_row_for_y(px(20.0), px(18.0), 3), Some(1));
        assert_eq!(visual_row_for_y(px(60.0), px(18.0), 3), None);
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
            kind: sola_document::BlockKind::Heading { level: 1 },
        };
        assert_eq!(focused.rendered_to_source(2), 2);
        assert_eq!(focused.source_to_rendered(2), 2);

        // Blurred block (Prefix hidden mapping)
        let blurred = EditorBlock {
            text: "H1".into(), // rendered
            runs: vec![],
            font_size: gpui::px(28.0),
            line_height: gpui::px(36.0),
            global_start: 0,
            source_len: 4, // "# H1"
            is_focused: false,
            kind: sola_document::BlockKind::Heading { level: 1 },
        };
        // Index 0 in "H1" is index 2 in "# H1"
        assert_eq!(blurred.rendered_to_source(0), 2);
        // Index 1 in "# H1" (the space) clamps to 0 in "H1"
        assert_eq!(blurred.source_to_rendered(1), 0);
    }
}
