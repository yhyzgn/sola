use gpui::{
    Font, FontFeatures, FontStyle, FontWeight, Hsla, Pixels, Point, SharedString, TextRun, Window,
    WrappedLine, px,
};
use sola_theme::Theme;

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

#[derive(Clone)]
struct VisualLineRef {
    global_start: usize,
    global_end: usize,
    wrapped_line_start: usize,
    row: usize,
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
    let current_visual_line = find_visual_line(&visual_lines, current_offset)?;
    let target_visual_line = current_visual_line.checked_add_signed(delta)?;
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
                y: line_height * target.row as f32,
            },
            line_height,
        )
        .unwrap_or_else(|index| index);

    Some(target.global_start + target_local.saturating_sub(target.wrapped_line_start))
}

fn collect_visual_lines(lines: &[WrappedLine]) -> Vec<VisualLineRef> {
    let mut visual = Vec::new();
    let mut global_base = 0;

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
        for (row, local_end) in boundaries.into_iter().enumerate() {
            visual.push(VisualLineRef {
                global_start: global_base + local_start,
                global_end: global_base + local_end,
                wrapped_line_start: local_start,
                row,
                line: line.clone(),
            });
            local_start = local_end;
        }

        global_base += line.text.len() + 1;
    }

    visual
}

fn find_visual_line(lines: &[VisualLineRef], offset: usize) -> Option<usize> {
    lines
        .iter()
        .position(|line| offset >= line.global_start && offset <= line.global_end)
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
}
