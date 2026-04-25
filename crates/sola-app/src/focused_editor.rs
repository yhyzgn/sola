use gpui::{
    App, Bounds, Element, ElementId, Font, FontFeatures, FontStyle, FontWeight, GlobalElementId,
    Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Point, SharedString, Style, TextAlign,
    TextRun, Window, WrappedLine, px,
};
use sola_document::highlighter::{HighlightKind, HighlightedSpan};
use sola_document::CursorState;
use sola_theme::{Theme, parse_hex_color};

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

    pub fn font_size_f32(&self) -> f32 {
        self.font_size.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrappedVisualLine {
    pub start: usize,
    pub end: usize,
    pub row: usize,
}

#[derive(Clone)]
struct VisualLineRef {
    global_start: usize,
    global_end: usize,
    wrapped_line_start: usize,
    local_row: usize,
    global_row: usize,
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
    lines: &[WrappedLine],
    current_offset: usize,
    delta: isize,
    line_height: Pixels,
) -> Option<usize> {
    let visual_lines = collect_visual_lines(lines);
    let current_visual_line = find_visual_line_ref(&visual_lines, current_offset)?;
    let target_global_row = shift_visual_row(
        visual_lines.get(current_visual_line)?.global_row,
        delta,
        visual_lines.len(),
    )?;
    let target_visual_line = visual_lines
        .iter()
        .position(|line| line.global_row == target_global_row)?;
    let current = visual_lines.get(current_visual_line)?;
    let target = visual_lines.get(target_visual_line)?;

    let current_local = current_offset.saturating_sub(current.global_start);
    let current_point = current
        .line
        .position_for_index(current.wrapped_line_start + current_local, line_height)?;

    let target_local = target
        .line
        .closest_index_for_position(
            Point {
                x: current_point.x,
                y: line_height * target.local_row as f32,
            },
            line_height,
        )
        .unwrap_or_else(|index| index);

    Some(target.global_start + target_local.saturating_sub(target.wrapped_line_start))
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

fn collect_visual_lines(lines: &[WrappedLine]) -> Vec<VisualLineRef> {
    let mut visual = Vec::new();
    let mut global_base = 0;
    let mut global_row = 0;

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
                wrapped_line_start: local_start,
                local_row,
                global_row,
                line: line.clone(),
            });
            local_start = local_end;
            global_row += 1;
        }

        global_base += line.text.len() + 1;
    }

    visual
}

pub fn visual_line_ranges(lines: &[WrappedLine]) -> Vec<WrappedVisualLine> {
    collect_visual_lines(lines)
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

fn visual_row_for_y(y: Pixels, line_height: Pixels, total_rows: usize) -> Option<usize> {
    let row = (y / line_height) as usize;
    (row < total_rows).then_some(row)
}

#[allow(dead_code)]
pub fn hit_test_visual_offset(
    lines: &[WrappedLine],
    point: Point<Pixels>,
    line_height: Pixels,
) -> Option<usize> {
    let visual_lines = collect_visual_lines(lines);
    let global_row = visual_row_for_y(point.y, line_height, visual_lines.len())?;
    let line = visual_lines
        .iter()
        .find(|line| line.global_row == global_row)?;

    let local = line
        .line
        .closest_index_for_position(
            Point {
                x: point.x,
                y: line_height * line.local_row as f32,
            },
            line_height,
        )
        .unwrap_or_else(|index| index);

    Some(line.global_start + local.saturating_sub(line.wrapped_line_start))
}

pub struct FocusedEditorElement {
    text: SharedString,
    style: FocusedEditorStyle,
    runs: Vec<TextRun>,
    cursor: Option<CursorState>,
    cursor_visible: bool,
    selection_color: Hsla,
    cursor_color: Hsla,
}

impl FocusedEditorElement {
    pub fn new(
        text: impl Into<SharedString>,
        style: FocusedEditorStyle,
        runs: Vec<TextRun>,
        cursor: Option<CursorState>,
        cursor_visible: bool,
        selection_color: Hsla,
        cursor_color: Hsla,
    ) -> Self {
        Self {
            text: text.into(),
            style,
            runs,
            cursor,
            cursor_visible,
            selection_color,
            cursor_color,
        }
    }
}

pub struct FocusedEditorState {
    lines: Vec<WrappedLine>,
}

impl Element for FocusedEditorElement {
    type RequestLayoutState = Vec<WrappedLine>;
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
        let lines = window
            .text_system()
            .shape_text(
                self.text.clone(),
                self.style.font_size,
                &self.runs,
                Some(wrap_width),
                None,
            )
            .unwrap_or_default()
            .into_vec();

        let layout_id = window.request_layout(style, None, cx);

        (layout_id, lines)
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
        let line_height = self.style.line_height;
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

            let visual_lines = collect_visual_lines(request_layout_state);
            for visual_line in visual_lines {
                let overlap_start = start.max(visual_line.global_start);
                let overlap_end = end.min(visual_line.global_end);

                if overlap_start < overlap_end {
                    let local_start =
                        visual_line.wrapped_line_start + (overlap_start - visual_line.global_start);
                    let local_end =
                        visual_line.wrapped_line_start + (overlap_end - visual_line.global_start);

                    let x_start = visual_line.line.unwrapped_layout.x_for_index(local_start);
                    let x_end = visual_line.line.unwrapped_layout.x_for_index(local_end);

                    let selection_bounds = Bounds {
                        origin: text_bounds.origin
                            + Point {
                                x: x_start,
                                y: line_height * visual_line.global_row as f32,
                            },
                        size: gpui::size(x_end - x_start, line_height),
                    };

                    window.paint_quad(gpui::fill(selection_bounds, self.selection_color));
                }
            }
        }

        // 2. Paint Text
        let mut y_offset = Pixels::ZERO;
        for line in request_layout_state.iter() {
            line.paint(
                text_bounds.origin + Point { x: Pixels::ZERO, y: y_offset },
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
            y_offset += line_height;
        }

        // 3. Paint Caret
        if let Some(cursor) = &self.cursor
            && self.cursor_visible
        {
            let visual_lines = collect_visual_lines(request_layout_state);
            if let Some(visual_line_idx) = find_visual_line_ref(&visual_lines, cursor.head) {
                let visual_line = &visual_lines[visual_line_idx];
                let local_offset =
                    visual_line.wrapped_line_start + (cursor.head - visual_line.global_start);

                let x = visual_line.line.unwrapped_layout.x_for_index(local_offset);

                let caret_bounds = Bounds {
                    origin: text_bounds.origin
                        + Point {
                            x,
                            y: line_height * visual_line.global_row as f32,
                        },
                    size: gpui::size(self.style.caret_width, line_height),
                };

                window.paint_quad(gpui::fill(caret_bounds, self.cursor_color));
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
}
