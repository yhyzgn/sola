use gpui::{
    App, Bounds, Element, ElementId, Font, FontFeatures, FontStyle, FontWeight,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Point,
    Style, TextRun, Window, px,
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
    pub fn from_theme(_theme: &Theme, config: &crate::config::AppConfig) -> Self {
        let font_size = px(config.editor_font_size);
        let line_height = font_size * config.editor_line_height;

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

pub fn approximate_editor_wrap_width(available_width: Pixels) -> Pixels {
    let padding = px(80.0);
    if available_width > padding {
        available_width - padding
    } else {
        px(120.0)
    }
}

pub struct FocusedEditorElement {
    blocks: Vec<EditorBlock>,
    typst_cache: HashMap<String, TypstAdapter>,
    style: FocusedEditorStyle,
    cursor: Option<CursorState>,
    cursor_visible: bool,
    selection_color: Hsla,
    cursor_color: Hsla,
    accent_color: Hsla,
    code_bg_color: Hsla,
    on_cursor_move: Option<Arc<dyn Fn(usize, bool, &mut Window, &mut App) + Send + Sync>>,
    on_toggle_checkbox: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>>,
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
        accent_color: Hsla,
        code_bg_color: Hsla,
    ) -> Self {
        Self {
            blocks,
            typst_cache,
            style,
            cursor,
            cursor_visible,
            selection_color,
            cursor_color,
            accent_color,
            code_bg_color,
            on_cursor_move: None,
            on_toggle_checkbox: None,
        }
    }

    pub fn on_cursor_move(
        mut self,
        callback: impl Fn(usize, bool, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_cursor_move = Some(Arc::new(callback));
        self
    }

    pub fn on_toggle_checkbox(
        mut self,
        callback: impl Fn(usize, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_toggle_checkbox = Some(Arc::new(callback));
        self
    }
}

#[derive(Clone, Debug)]
pub struct EditorObject {
    pub start: usize,
    pub end: usize,
    pub kind: EditorObjectKind,
}

#[derive(Clone, Debug)]
pub enum EditorObjectKind {
    Math { cache_key: String },
    Checkbox { checked: bool },
}

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

#[derive(Clone, Debug)]
pub struct MappingSegment {
    pub rendered_start: usize,
    pub source_start: usize,
}

#[derive(Clone)]
pub struct EditorBlock {
    pub text: String,
    pub runs: Vec<TextRun>,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub indentation: Pixels,
    pub global_start: usize,
    pub mapping: Vec<MappingSegment>,
    pub is_focused: bool,
    pub kind: BlockKind,
    pub objects: Vec<EditorObject>,
}

impl EditorBlock {
    pub fn source_to_rendered(&self, source_local: usize) -> usize {
        if self.is_focused {
            return source_local;
        }

        // Check if we are inside an object
        for obj in &self.objects {
            if source_local >= obj.start && source_local < obj.end {
                // Find the mapping segment for this object
                for segment in &self.mapping {
                    if segment.source_start == obj.start {
                        return segment.rendered_start;
                    }
                }
            }
        }

        // Find the segment that contains this source offset
        let mut best_segment = None;
        for segment in &self.mapping {
            if segment.source_start <= source_local {
                best_segment = Some(segment);
            } else {
                break;
            }
        }

        if let Some(segment) = best_segment {
            let offset_in_segment = source_local - segment.source_start;
            segment.rendered_start + offset_in_segment
        } else {
            0
        }
    }

    pub fn rendered_to_source(&self, rendered_local: usize) -> usize {
        if self.is_focused {
            return rendered_local;
        }

        // Find the segment that contains this rendered offset
        let mut best_segment = None;
        for segment in &self.mapping {
            if segment.rendered_start <= rendered_local {
                best_segment = Some(segment);
            } else {
                break;
            }
        }

        if let Some(segment) = best_segment {
            let offset_in_segment = rendered_local - segment.rendered_start;
            segment.source_start + offset_in_segment
        } else {
            0
        }
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
    let mut current_list_index = 0;

    for (i, block) in doc.blocks().iter().enumerate() {
        let is_focused = focused_block_idx == Some(i);

        // Reset list index if not in a continuous list
        if !matches!(block.kind, BlockKind::ListItem { ordered: true }) {
            current_list_index = 0;
        }

        let (text, font_size, line_height, runs, objects, mapping) = if is_focused {
            let highlighter = SyntaxHighlighter::new_rust();
            let spans = highlighter.highlight(&block.source);
            let runs = spans_to_runs(&spans, style, theme);
            (
                block.source.clone(),
                style.font_size,
                style.line_height,
                runs,
                Vec::new(),
                Vec::new(),
            )
        } else {
            let (text, fs, lh, runs, objects, mapping, next_index) = generate_rich_text(&block.source, &block.kind, theme, current_list_index);
            current_list_index = next_index;
            (text, fs, lh, runs, objects, mapping)
        };

        let indentation = match &block.kind {
            BlockKind::ListItem { .. } | BlockKind::Quote => px(24.0),
            _ => px(0.0),
        };

        let source_len = block.source.len();
        blocks.push(EditorBlock {
            text,
            runs,
            font_size,
            line_height,
            indentation,
            global_start: current_global,
            mapping,
            is_focused,
            kind: block.kind.clone(),
            objects,
        });

        current_global += source_len + 2;
    }

    blocks
}


fn generate_rich_text(
    source: &str,
    kind: &BlockKind,
    theme: &Theme,
    start_list_index: usize,
) -> (String, Pixels, Pixels, Vec<TextRun>, Vec<EditorObject>, Vec<MappingSegment>, usize) {
    let mut text = String::new();
    let mut runs = Vec::new();
    let mut objects = Vec::new();
    let mut mapping = Vec::new();
    let mut current_list_index = start_list_index;

    // 1. Determine prefix to strip and visual marker to add
    let mut source_prefix_len = 0;
    let mut visual_marker = String::new();
    let mut checkbox = None;
    let marker_color = rgb_hex(&theme.palette.accent);

    match kind {
        BlockKind::Heading { .. } => {
            let trimmed = source.trim_start();
            let hashes = trimmed.chars().take_while(|c| *c == '#').count();
            source_prefix_len = (source.len() - trimmed.len()) + hashes + if trimmed[hashes..].starts_with(' ') { 1 } else { 0 };
        }
        BlockKind::ListItem { ordered } => {
            let trimmed = source.trim_start();
            let mut internal_prefix = 0;
            if *ordered {
                if let Some(dot_pos) = trimmed.find(". ") {
                    internal_prefix = dot_pos + 2;
                    current_list_index += 1;
                    visual_marker = format!("{}. ", current_list_index);
                } else {
                    // Fallback for malformed ordered list
                    internal_prefix = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
                    if trimmed[internal_prefix..].starts_with('.') { internal_prefix += 1; }
                    if trimmed[internal_prefix..].starts_with(' ') { internal_prefix += 1; }
                    current_list_index += 1;
                    visual_marker = format!("{}. ", current_list_index);
                }
            } else {
                if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
                    internal_prefix = 2;
                    // Check for task list in the remaining part of the trimmed string
                    let after_bullet = &trimmed[2..];
                    if after_bullet.starts_with("[ ] ") {
                        checkbox = Some(false);
                        internal_prefix += 4;
                    } else if after_bullet.starts_with("[x] ") || after_bullet.starts_with("[X] ") {
                        checkbox = Some(true);
                        internal_prefix += 4;
                    } else {
                        visual_marker = "• ".to_string();
                    }
                } else if trimmed.starts_with("-") || trimmed.starts_with("*") || trimmed.starts_with("+") {
                    // Handle list marker without space
                    internal_prefix = 1;
                    visual_marker = "• ".to_string();
                }
            }
            source_prefix_len = (source.len() - trimmed.len()) + internal_prefix;
        }
        BlockKind::Quote => {
            let trimmed = source.trim_start();
            source_prefix_len = (source.len() - trimmed.len()) + if trimmed.starts_with("> ") { 2 } else if trimmed.starts_with(">") { 1 } else { 0 };
        }
        _ => {}
    }

    // 2. Add visual marker or checkbox to the output
    if let Some(checked) = checkbox {
        mapping.push(MappingSegment { rendered_start: 0, source_start: 0 });
        
        objects.push(EditorObject {
            start: (source.len() - source.trim_start().len()) + 2, // After bullet
            end: source_prefix_len,
            kind: EditorObjectKind::Checkbox { checked },
        });

        text.push('\u{FFFC}'); // Placeholder for checkbox
        text.push(' ');
        runs.push(TextRun {
            len: '\u{FFFC}'.len_utf8() + 1,
            font: Font {
                family: "System UI".into(),
                features: FontFeatures::default(),
                fallbacks: None,
                weight: FontWeight::BOLD,
                style: FontStyle::default(),
            },
            color: marker_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    } else if !visual_marker.is_empty() {
        mapping.push(MappingSegment { rendered_start: 0, source_start: 0 });
        text.push_str(&visual_marker);
        runs.push(TextRun {
            len: visual_marker.len(),
            font: Font {
                family: "System UI".into(),
                features: FontFeatures::default(),
                fallbacks: None,
                weight: FontWeight::BOLD,
                style: FontStyle::default(),
            },
            color: marker_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    // 3. Parse the REST of the source
    let stripped_source = if source_prefix_len < source.len() {
        &source[source_prefix_len..]
    } else {
        ""
    };

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_MATH);
    let parser = Parser::new_ext(stripped_source, options).into_offset_iter();

    let mut current_weight;
    let mut current_style = FontStyle::default();
    let current_font_family: String = "System UI".into();
    let mut current_color = rgb_hex(&theme.palette.text_primary);
    let mut is_strikethrough = false;
    let mut is_link = false;
    let is_blockquote = matches!(kind, BlockKind::Quote);

    // Heading base styles
    let (size_mult, base_weight) = match kind {
        BlockKind::Heading { level: 1 } => (2.0, FontWeight::BOLD),
        BlockKind::Heading { level: 2 } => (1.5, FontWeight::BOLD),
        BlockKind::Heading { level: 3 } => (1.25, FontWeight::BOLD),
        BlockKind::Heading { .. } => (1.1, FontWeight::BOLD),
        _ => (1.0, FontWeight::NORMAL),
    };
    current_weight = base_weight;
    if is_blockquote {
        current_color = rgb_hex(&theme.palette.text_muted);
    }

    let base_size = theme.typography.body_size as f32;
    let font_size = px(base_size * size_mult);
    let line_height = font_size * 1.5;

    for (event, range) in parser {
        let global_range_start = source_prefix_len + range.start;
        let global_range_end = source_prefix_len + range.end;

        match event {
            Event::Start(Tag::Strong) => current_weight = FontWeight::BOLD,
            Event::End(TagEnd::Strong) => current_weight = base_weight,
            Event::Start(Tag::Emphasis) => current_style = FontStyle::Italic,
            Event::End(TagEnd::Emphasis) => current_style = FontStyle::Normal,
            Event::Start(Tag::Strikethrough) => is_strikethrough = true,
            Event::End(TagEnd::Strikethrough) => is_strikethrough = false,
            Event::Start(Tag::Link { .. }) => is_link = true,
            Event::End(TagEnd::Link) => is_link = false,
            Event::Start(Tag::Image { title, .. }) => {
                mapping.push(MappingSegment {
                    rendered_start: text.len(),
                    source_start: global_range_start,
                });
                let label = if title.is_empty() {
                    "[Image]".to_string()
                } else {
                    format!("[Image: {}]", title)
                };
                text.push_str(&label);
                runs.push(TextRun {
                    len: label.len(),
                    font: Font {
                        family: current_font_family.clone().into(),
                        features: FontFeatures::default(),
                        fallbacks: None,
                        weight: FontWeight::BOLD,
                        style: FontStyle::default(),
                    },
                    color: rgb_hex(&theme.palette.accent),
                    background_color: Some(rgb_hex(&theme.palette.code_background)),
                    underline: None,
                    strikethrough: None,
                });
            }
            Event::InlineMath(t) => {
                mapping.push(MappingSegment {
                    rendered_start: text.len(),
                    source_start: global_range_start,
                });
                
                objects.push(EditorObject {
                    start: global_range_start,
                    end: global_range_end,
                    kind: EditorObjectKind::Math { cache_key: format!("math::{}", t) },
                });
                
                text.push('\u{FFFC}');
                runs.push(TextRun {
                    len: '\u{FFFC}'.len_utf8(),
                    font: Font {
                        family: current_font_family.clone().into(),
                        features: FontFeatures::default(),
                        fallbacks: None,
                        weight: current_weight,
                        style: current_style,
                    },
                    color: current_color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                });
            }
            Event::Code(c) => {
                mapping.push(MappingSegment {
                    rendered_start: text.len(),
                    source_start: global_range_start,
                });
                text.push_str(&c);
                runs.push(TextRun {
                    len: c.len(),
                    font: Font {
                        family: "JetBrains Mono".into(),
                        features: FontFeatures::default(),
                        fallbacks: None,
                        weight: current_weight,
                        style: current_style,
                    },
                    color: rgb_hex(&theme.syntax.string),
                    background_color: Some(rgb_hex(&theme.palette.code_background)),
                    underline: None,
                    strikethrough: is_strikethrough.then_some(gpui::StrikethroughStyle::default()),
                });
            }
            Event::Text(t) => {
                mapping.push(MappingSegment {
                    rendered_start: text.len(),
                    source_start: global_range_start,
                });
                
                text.push_str(&t);
                runs.push(TextRun {
                    len: t.len(),
                    font: Font {
                        family: current_font_family.clone().into(),
                        features: FontFeatures::default(),
                        fallbacks: None,
                        weight: current_weight,
                        style: current_style,
                    },
                    color: if is_link { rgb_hex(&theme.palette.accent) } else { current_color },
                    background_color: None,
                    underline: is_link.then_some(gpui::UnderlineStyle::default()),
                    strikethrough: is_strikethrough.then_some(gpui::StrikethroughStyle::default()),
                });
            }
            Event::SoftBreak | Event::HardBreak => {
                text.push(' ');
                runs.push(TextRun {
                    len: 1,
                    font: Font {
                        family: current_font_family.clone().into(),
                        features: FontFeatures::default(),
                        fallbacks: None,
                        weight: current_weight,
                        style: current_style,
                    },
                    color: current_color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                });
            }
            _ => {}
        }
    }

    // If text is empty (e.g. empty block), add a dummy run
    if text.is_empty() {
        text.push(' ');
        runs.push(TextRun {
            len: 1,
            font: Font {
                family: current_font_family.into(),
                features: FontFeatures::default(),
                fallbacks: None,
                weight: current_weight,
                style: current_style,
            },
            color: current_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
        mapping.push(MappingSegment {
            rendered_start: 0,
            source_start: 0,
        });
    }

    (text, font_size, line_height, runs, objects, mapping, current_list_index)
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
        let available_width = window.bounds().size.width.min(px(900.0));
        let wrap_width = approximate_editor_wrap_width(available_width);

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
                        let block = &self.blocks[line.block_index];
                        let rendered_start = block.source_to_rendered(overlap_start - block.global_start);
                        let rendered_end = block.source_to_rendered(overlap_end - block.global_start);
                        
                        let x_start = line.wrapped_line.position_for_index(rendered_start, line.bounds.size.height).map(|p| p.x).unwrap_or(Pixels::ZERO);
                        let x_end = line.wrapped_line.position_for_index(rendered_end, line.bounds.size.height).map(|p| p.x).unwrap_or(line.bounds.size.width);

                        let rect = Bounds {
                            origin: text_bounds.origin + line.bounds.origin + Point { x: x_start, y: Pixels::ZERO },
                            size: gpui::size(x_end - x_start, line.bounds.size.height),
                        };
                        window.paint_quad(gpui::fill(rect, self.selection_color));
                    }
                }
            }
        }

        // 2. Paint Text and Objects
        for line in &visual_doc.lines {
            let block = &self.blocks[line.block_index];

            // Paint block-level decorations
            if !block.is_focused {
                match &block.kind {
                    BlockKind::Quote => {
                        let bar_width = px(4.0);
                        let bar_rect = Bounds {
                            origin: text_bounds.origin + Point { x: px(0.0), y: line.bounds.origin.y },
                            size: gpui::size(bar_width, line.bounds.size.height),
                        };
                        window.paint_quad(gpui::fill(bar_rect, self.accent_color));
                    }
                    BlockKind::CodeFence { .. } | BlockKind::MathBlock | BlockKind::TypstBlock => {
                        let bg_rect = Bounds {
                            origin: text_bounds.origin + line.bounds.origin,
                            size: line.bounds.size,
                        };
                        window.paint_quad(gpui::fill(bg_rect, self.code_bg_color));
                    }
                    _ => {}
                }
            }

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
                let obj_bounds = Bounds {
                    origin: text_bounds.origin + line.bounds.origin + *offset,
                    size: gpui::size(obj.width, obj.height),
                };

                match &obj.kind {
                    EditorObjectKind::Math { cache_key } => {
                        if let Some(TypstAdapter::Rendered { svg }) = self.typst_cache.get(cache_key) {
                            let _ = window.paint_svg(
                                obj_bounds,
                                svg.clone().into(),
                                gpui::TransformationMatrix::default(),
                                gpui::white(),
                                cx,
                            );
                        }
                    }
                    EditorObjectKind::Checkbox { checked } => {
                        let padding = px(2.0);
                        let check_bounds = Bounds {
                            origin: obj_bounds.origin + Point { x: padding, y: padding },
                            size: gpui::size(obj_bounds.size.width - padding * 2.0, obj_bounds.size.height - padding * 2.0),
                        };

                        // Draw border (using accent color)
                        window.paint_quad(gpui::fill(check_bounds, self.accent_color));
                        
                        // Draw inside (white)
                        let inner_check_bounds = Bounds {
                            origin: check_bounds.origin + Point { x: px(1.0), y: px(1.0) },
                            size: gpui::size(check_bounds.size.width - px(2.0), check_bounds.size.height - px(2.0)),
                        };
                        window.paint_quad(gpui::fill(inner_check_bounds, gpui::white()));

                        if *checked {
                            // Draw a simple checkmark (filled box for now)
                            let inner_padding = px(2.0);
                            let mark_bounds = Bounds {
                                origin: inner_check_bounds.origin + Point { x: inner_padding, y: inner_padding },
                                size: gpui::size(inner_check_bounds.size.width - inner_padding * 2.0, inner_check_bounds.size.height - inner_padding * 2.0),
                            };
                            window.paint_quad(gpui::fill(mark_bounds, self.accent_color));
                        }

                        if let Some(on_toggle) = &self.on_toggle_checkbox {
                            let on_toggle = on_toggle.clone();
                            let global_offset = obj.global_offset;
                            let bounds = check_bounds;
                            window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, cx| {
                                if phase == gpui::DispatchPhase::Bubble && bounds.contains(&event.position) {
                                    on_toggle(global_offset, window, cx);
                                }
                            });
                        }
                    }
                }
            }
        }

        // 3. Paint Caret
        if let Some(cursor) = &self.cursor {
            if self.cursor_visible {
                for line in &visual_doc.lines {
                    if cursor.head >= line.global_start && cursor.head <= line.global_end {
                        let block = &self.blocks[line.block_index];
                        let rendered_head = block.source_to_rendered(cursor.head - block.global_start);
                        let x = line.wrapped_line.position_for_index(rendered_head, line.bounds.size.height).map(|p| p.x).unwrap_or(Pixels::ZERO);

                        let caret_bounds = Bounds {
                            origin: text_bounds.origin + line.bounds.origin + Point { x, y: Pixels::ZERO },
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
    use crate::config::AppConfig;
    use super::*;
    use sola_theme::Theme;

    #[test]
    fn editor_style_derives_compact_code_metrics_from_theme() {
        let style = FocusedEditorStyle::from_theme(&Theme::sola_dark(), &AppConfig::default());

        assert_eq!(style.font_family, "JetBrains Mono");
        assert_eq!(style.font_size, px(16.0));
        assert_eq!(style.line_height, px(19.2));
        assert_eq!(style.padding_x, px(40.0));
        assert_eq!(style.padding_y, px(20.0));
        assert_eq!(style.caret_width, px(2.0));
    }

    #[test]
    fn approximate_wrap_width_reserves_sidebar_and_padding_budget() {
        assert_eq!(approximate_editor_wrap_width(px(1000.0)), px(920.0));
        assert_eq!(approximate_editor_wrap_width(px(300.0)), px(220.0));
    }

    #[test]
    fn test_editor_block_generation() {
        let doc = DocumentModel::from_markdown("# H1\n\nText");
        let theme = Theme::sola_dark();
        let style = FocusedEditorStyle::from_theme(&theme, &AppConfig::default());

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
            indentation: gpui::px(0.0),
            global_start: 0,
            mapping: vec![],
            is_focused: true,
            kind: BlockKind::Heading { level: 1 },
            objects: vec![],
        };
        assert_eq!(focused.rendered_to_source(2), 2);
        assert_eq!(focused.source_to_rendered(2), 2);

        // Blurred block (with inline math mapping)
        let blurred = EditorBlock {
            text: "Hello \u{FFFC} world".into(),
            runs: vec![],
            font_size: gpui::px(14.0),
            line_height: gpui::px(20.0),
            indentation: gpui::px(0.0),
            global_start: 0,
            mapping: vec![
                MappingSegment { rendered_start: 0, source_start: 0 },
                MappingSegment { rendered_start: 6, source_start: 6 },
                MappingSegment { rendered_start: 7, source_start: 14 },
            ],
            is_focused: false,
            kind: BlockKind::Paragraph,
            objects: vec![EditorObject {
                start: 6,
                end: 14,
                kind: EditorObjectKind::Math { cache_key: "math::e=mc^2".into() },
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
        let style = FocusedEditorStyle::from_theme(&theme, &AppConfig::default());
        
        // global_cursor = None means no block is focused
        let blocks = generate_editor_blocks(&doc, None, &style, &theme);
        
        assert_eq!(blocks.len(), 1);
        let block = &blocks[0];
        
        // Expected text: "Hello \u{FFFC} world"
        assert_eq!(block.text, "Hello \u{FFFC} world");
        assert_eq!(block.objects.len(), 1);
        assert_eq!(block.objects[0].start, 6); // "$" at index 6
        assert_eq!(block.objects[0].end, 14); // after second "$"
    }
}
