use gpui::{
    AppContext, Application, AsyncApp, Bounds, Context, Div, FocusHandle, FontWeight, Hsla, Image,
    ImageFormat, InteractiveElement, IntoElement, MouseButton, ParentElement, Render,
    StatefulInteractiveElement, Styled, WeakEntity, Window, WindowBounds, WindowOptions, div, img,
    px, rgb, size,
};
use sola_core::{APP_NAME, APP_TAGLINE, ROADMAP_PHASES, sample_markdown};
use sola_document::highlighter::{HighlightKind, SyntaxHighlighter};
use sola_document::{
    BlockKind, CursorState, DocumentBlock, DocumentModel, HtmlAdapter, HtmlNode, TypstAdapter,
};
use sola_theme::{Theme, parse_hex_color};
use sola_typst::{RenderKind, TypstError, compile_to_svg};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
#[cfg(target_os = "linux")]
use std::{
    env,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};

pub fn run() {
    #[cfg(target_os = "linux")]
    if let Err(message) = ensure_linux_display_backend() {
        eprintln!("{message}");
        return;
    }

    Application::new().run(|cx| {
        cx.on_window_closed(|cx| cx.quit()).detach();

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1440.0), px(960.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| SolaRoot::new(cx)),
        )
        .expect("open GPUI window");
    });
}

struct SolaRoot {
    focus_handle: FocusHandle,
    theme_mode: ThemeMode,
    theme: Theme,
    document: DocumentModel,
    highlighter: SyntaxHighlighter,
    typst_cache: HashMap<String, TypstAdapter>,
    typst_in_flight: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeMode {
    Dark,
    Light,
}

impl SolaRoot {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            theme_mode: ThemeMode::Dark,
            theme: Theme::sola_dark(),
            document: DocumentModel::from_markdown(sample_markdown()),
            highlighter: SyntaxHighlighter::new_rust(),
            typst_cache: HashMap::new(),
            typst_in_flight: HashSet::new(),
        }
    }

    fn sync_theme(&mut self) {
        self.theme = match self.theme_mode {
            ThemeMode::Dark => Theme::sola_dark(),
            ThemeMode::Light => Theme::sola_light(),
        };
    }

    fn toggle_theme(&mut self) {
        self.theme_mode = self.theme_mode.toggle();
        self.sync_theme();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> Div {
        let toggle_theme = action_button(
            format!("theme: {}", self.theme_mode.label()),
            &self.theme,
            true,
        )
        .id("toggle-theme")
        .on_click(cx.listener(|this, _event, _window, cx| {
            this.toggle_theme();
            cx.notify();
        }));

        div()
            .flex()
            .justify_between()
            .items_center()
            .p(px(20.0))
            .border_b_1()
            .border_color(rgb_hex(&self.theme.palette.panel_border))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.theme.typography.title_size as f32))
                            .font_weight(FontWeight::BOLD)
                            .child(APP_NAME),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(rgb_hex(&self.theme.palette.text_muted))
                            .child(APP_TAGLINE),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(toggle_theme)
                    .child(pill("workspace", format!("{} crates", 4), &self.theme))
                    .child(pill(
                        "focused block",
                        format!("#{}", self.document.focused_block() + 1),
                        &self.theme,
                    ))
                    .child(pill("roadmap", "phase 1 / 2".to_string(), &self.theme)),
            )
    }

    fn render_sidebar(&self) -> Div {
        let outline = self.document.outline().iter().fold(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(section_title("Document outline", &self.theme)),
            |sidebar, entry| {
                sidebar.child(
                    div()
                        .text_size(px(13.0))
                        .text_color(rgb_hex(&self.theme.palette.text_muted))
                        .pl(px((entry.level.saturating_sub(1) as f32) * 14.0))
                        .child(format!("H{} · {}", entry.level, entry.title)),
                )
            },
        );

        let roadmap = ROADMAP_PHASES.iter().fold(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(section_title("Roadmap", &self.theme)),
            |sidebar, phase| {
                sidebar.child(
                    div()
                        .text_size(px(13.0))
                        .text_color(rgb_hex(&self.theme.palette.text_muted))
                        .child(*phase),
                )
            },
        );

        div()
            .w(px(320.0))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(18.0))
            .p(px(18.0))
            .bg(rgb_hex(&self.theme.palette.panel_background))
            .border_r_1()
            .border_color(rgb_hex(&self.theme.palette.panel_border))
            .child(self.render_stats_card())
            .child(outline)
            .child(roadmap)
    }

    fn render_stats_card(&self) -> Div {
        let stats = self.document.stats();

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(16.0))
            .bg(rgb_hex(&self.theme.palette.app_background))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgb_hex(&self.theme.palette.panel_border))
            .child(section_title("Prototype status", &self.theme))
            .child(meta_line(
                "blocks",
                self.document.blocks().len().to_string(),
                &self.theme,
            ))
            .child(meta_line(
                "headings",
                stats.headings.to_string(),
                &self.theme,
            ))
            .child(meta_line(
                "paragraphs",
                stats.paragraphs.to_string(),
                &self.theme,
            ))
            .child(meta_line(
                "lists",
                stats.list_items.to_string(),
                &self.theme,
            ))
            .child(meta_line("quotes", stats.quotes.to_string(), &self.theme))
            .child(meta_line(
                "code",
                stats.code_blocks.to_string(),
                &self.theme,
            ))
    }

    fn render_document_surface(&mut self, cx: &mut Context<Self>) -> Div {
        self.trigger_typst_renders(cx);
        let blocks = self.document.blocks().iter().enumerate().fold(
            div().flex().flex_col().gap(px(14.0)).p(px(24.0)),
            |surface, (index, block)| surface.child(self.render_block(index, block, cx)),
        );

        let previous_button = action_button(
            "← previous block".to_string(),
            &self.theme,
            self.document.focused_block() > 0,
        )
        .id("previous-block")
        .on_click(cx.listener(|this, _event, _window, cx| {
            if this.document.focus_previous() {
                cx.notify();
            }
        }));

        let next_button = action_button(
            "next block →".to_string(),
            &self.theme,
            self.document.focused_block() + 1 < self.document.block_count(),
        )
        .id("next-block")
        .on_click(cx.listener(|this, _event, _window, cx| {
            if this.document.focus_next() {
                cx.notify();
            }
        }));

        let focused_summary = self
            .document
            .focused_block_ref()
            .map(|block| block.rendered.clone())
            .unwrap_or_else(|| "no focused block".to_string());
        let draft_label = if self.document.focused_has_draft() {
            "draft pending"
        } else {
            "source synced"
        };
        let insert_button = action_button("insert paragraph".to_string(), &self.theme, true)
            .id("insert-paragraph")
            .on_click(cx.listener(|this, _event, _window, cx| {
                if this.document.insert_paragraph_after_focused(
                    "A new paragraph block inserted by the structure editing prototype.",
                ) {
                    cx.notify();
                }
            }));

        let duplicate_button = action_button("duplicate block".to_string(), &self.theme, true)
            .id("duplicate-block")
            .on_click(cx.listener(|this, _event, _window, cx| {
                if this.document.duplicate_focused_block() {
                    cx.notify();
                }
            }));

        let delete_button = action_button(
            "delete block".to_string(),
            &self.theme,
            self.document.block_count() > 1,
        )
        .id("delete-block")
        .on_click(cx.listener(|this, _event, _window, cx| {
            if this.document.delete_focused_block() {
                cx.notify();
            }
        }));

        let undo_button = action_button("undo".to_string(), &self.theme, self.document.can_undo())
            .id("undo")
            .on_click(cx.listener(|this, _event, _window, cx| {
                if this.document.undo() {
                    cx.notify();
                }
            }));

        let redo_button = action_button("redo".to_string(), &self.theme, self.document.can_redo())
            .id("redo")
            .on_click(cx.listener(|this, _event, _window, cx| {
                if this.document.redo() {
                    cx.notify();
                }
            }));

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .p(px(24.0))
                    .border_b_1()
                    .border_color(rgb_hex(&self.theme.palette.panel_border))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(section_title(
                                "Dual-state engine prototype",
                                &self.theme,
                            ))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb_hex(&self.theme.palette.text_muted))
                                    .child("Blurred blocks render their formatted summary; the focused block expands into raw Markdown source. Click another block to move focus."),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(10.0))
                                    .items_center()
                                    .child(undo_button)
                                    .child(redo_button)
                                    .child(previous_button)
                                    .child(next_button)
                                    .child(pill(
                                        "block summary",
                                        truncate_for_pill(&focused_summary, 40),
                                        &self.theme,
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(10.0))
                                    .items_center()
                                    .child(insert_button)
                                    .child(duplicate_button)
                                    .child(delete_button),
                            )
                            .child(pill(
                                "source state",
                                draft_label.to_string(),
                                &self.theme,
                            ))
                            .child(shortcut_legend(&self.theme)),
                    ),
            )
            .child(
                div()
                    .id("document-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(blocks),
            )
    }

    fn render_block(
        &self,
        index: usize,
        block: &DocumentBlock,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_focused = self.document.focused_block() == index;

        let block_container = div()
            .id(("block-container", index))
            .flex()
            .flex_row()
            .gap(px(12.0))
            .p(px(8.0))
            .cursor_pointer()
            .track_focus(&self.focus_handle);

        // Subtle focused indicator (accent color line on the left)
        let indicator = if is_focused {
            div()
                .w(px(2.0))
                .bg(rgb_hex(&self.theme.palette.accent))
                .rounded_full()
        } else {
            div().w(px(2.0))
        };

        let content = if is_focused {
            div()
                .flex_1()
                .on_key_down(cx.listener(|this, event, _window, cx| {
                    if this.handle_focused_key_down(event) {
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .p(px(8.0))
                        .bg(rgb_hex(&self.theme.palette.code_background))
                        .rounded(px(8.0))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                                let end = this.document.focused_text().map(str::len).unwrap_or(0);
                                let changed =
                                    this.document.set_focused_cursor(end, event.modifiers.shift);
                                cx.stop_propagation();
                                if changed {
                                    cx.notify();
                                }
                            }),
                        )
                        .child(self.render_highlighted_text(
                            self.document.focused_text().unwrap_or(&block.source),
                            13.0,
                            self.document.focused_cursor(),
                            Some(cx),
                        )),
                )
        } else {
            div().flex_1().child(self.render_blurred_content(block))
        };

        block_container
            .child(indicator)
            .child(content)
            .on_click(cx.listener(move |this, _event, window, cx| {
                if this.document.focused_block() == index {
                    return;
                }

                // Auto-apply draft before moving focus
                if this.document.focused_has_draft() {
                    this.document.apply_focused_draft();
                }

                if this.document.focus_block(index) {
                    window.focus(&this.focus_handle);
                    cx.notify();
                }
            }))
    }

    fn render_blurred_content(&self, block: &DocumentBlock) -> Div {
        match &block.kind {
            BlockKind::Heading { level } => div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb_hex(&self.theme.palette.accent))
                        .child(format!("Heading level {}", level)),
                )
                .child(
                    div()
                        .text_size(px(match level {
                            1 => 28.0,
                            2 => 24.0,
                            3 => 20.0,
                            _ => 18.0,
                        }))
                        .font_weight(FontWeight::BOLD)
                        .child(block.rendered.clone()),
                ),
            BlockKind::Paragraph => {
                if block.typst.is_some() {
                    self.render_typst_preview(block, "Paragraph")
                } else {
                    self.render_textual_block(
                        block,
                        self.theme.typography.body_size as f32,
                        &self.theme.palette.text_primary,
                    )
                }
            }
            BlockKind::ListItem { ordered } => div()
                .flex()
                .gap(px(10.0))
                .child(
                    div()
                        .text_color(rgb_hex(&self.theme.palette.accent))
                        .font_weight(FontWeight::BOLD)
                        .child(if *ordered { "1." } else { "•" }),
                )
                .child(if block.typst.is_some() {
                    self.render_typst_preview(block, "List item")
                } else {
                    self.render_textual_block(
                        block,
                        self.theme.typography.body_size as f32,
                        &self.theme.palette.text_primary,
                    )
                }),
            BlockKind::Quote => div()
                .pl(px(14.0))
                .border_l_2()
                .border_color(rgb_hex(&self.theme.palette.accent))
                .child(if block.typst.is_some() {
                    self.render_typst_preview(block, "Quote")
                } else {
                    self.render_textual_block(
                        block,
                        self.theme.typography.body_size as f32,
                        &self.theme.palette.text_muted,
                    )
                }),
            BlockKind::CodeFence { language } => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb_hex(&self.theme.palette.text_muted))
                        .child(format!(
                            "Code fence{}",
                            language
                                .as_ref()
                                .map(|lang| format!(" · {}", lang))
                                .unwrap_or_default()
                        )),
                )
                .child(
                    div()
                        .p(px(14.0))
                        .bg(rgb_hex(&self.theme.palette.code_background))
                        .rounded(px(10.0))
                        .child(self.render_highlighted_text(&block.rendered, 13.0, None, None)),
                ),
            BlockKind::MathBlock => self.render_typst_preview(block, "Math block"),
            BlockKind::TypstBlock => self.render_typst_preview(block, "Typst block"),
        }
    }

    fn render_typst_preview(&self, block: &DocumentBlock, label: &str) -> Div {
        let preview_height = match block.kind {
            BlockKind::MathBlock | BlockKind::TypstBlock => 160.0,
            BlockKind::Heading { .. }
            | BlockKind::Paragraph
            | BlockKind::ListItem { .. }
            | BlockKind::Quote
            | BlockKind::CodeFence { .. } => 56.0,
        };

        match block.typst.as_ref() {
            Some(TypstAdapter::Pending) => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb_hex(&self.theme.palette.text_muted))
                        .child(format!("{label} · rendering")),
                )
                .child(
                    div()
                        .p(px(14.0))
                        .bg(rgb_hex(&self.theme.palette.code_background))
                        .rounded(px(10.0))
                        .text_size(px(13.0))
                        .text_color(rgb_hex(&self.theme.palette.text_muted))
                        .child("Rendering Typst preview..."),
                ),
            Some(TypstAdapter::Rendered { svg }) => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb_hex(&self.theme.palette.text_muted))
                        .child(format!("{label} · rendered")),
                )
                .child(
                    div()
                        .p(px(14.0))
                        .bg(rgb_hex(&self.theme.palette.code_background))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(rgb_hex(&self.theme.palette.panel_border))
                        .child(
                            img(Arc::new(Image::from_bytes(
                                ImageFormat::Svg,
                                svg.as_bytes().to_vec(),
                            )))
                            .w_full()
                            .h(px(preview_height)),
                        ),
                ),
            Some(TypstAdapter::Error { message }) => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb_hex(&self.theme.palette.text_muted))
                        .child(format!("{label} · error")),
                )
                .child(
                    div()
                        .p(px(14.0))
                        .bg(rgb_hex(&self.theme.palette.code_background))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(rgb_hex(&self.theme.palette.panel_border))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(rgb_hex("#ff6b6b"))
                                .child(message.clone()),
                        )
                        .child(
                            div()
                                .mt(px(10.0))
                                .text_size(px(12.0))
                                .text_color(rgb_hex(&self.theme.palette.text_muted))
                                .child(block.rendered.clone()),
                        ),
                ),
            None => div()
                .text_size(px(self.theme.typography.body_size as f32))
                .text_color(rgb_hex(&self.theme.palette.text_primary))
                .child(block.rendered.clone()),
        }
    }

    fn render_textual_block(&self, block: &DocumentBlock, default_size: f32, color: &str) -> Div {
        match &block.html {
            Some(HtmlAdapter::Adapted { nodes }) => {
                self.render_html_nodes(nodes, default_size, color)
            }
            Some(HtmlAdapter::Unsupported { raw }) => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(pill(
                    "html adapter",
                    "degraded unsupported html".to_string(),
                    &self.theme,
                ))
                .child(
                    div()
                        .p(px(14.0))
                        .bg(rgb_hex(&self.theme.palette.code_background))
                        .rounded(px(10.0))
                        .text_size(px(13.0))
                        .text_color(rgb_hex(&self.theme.palette.text_muted))
                        .child(raw.clone()),
                ),
            None => div()
                .text_size(px(default_size))
                .text_color(rgb_hex(color))
                .child(block.rendered.clone()),
        }
    }

    fn render_html_nodes(&self, nodes: &[HtmlNode], default_size: f32, default_color: &str) -> Div {
        nodes.iter().fold(
            div().flex().flex_wrap().items_center().gap(px(6.0)),
            |content, node| match node {
                HtmlNode::Text(text) => content.child(
                    div()
                        .text_size(px(default_size))
                        .text_color(rgb_hex(default_color))
                        .child(text.clone()),
                ),
                HtmlNode::StyledText(styled) => {
                    let color = styled
                        .color
                        .as_deref()
                        .filter(|value| parse_hex_color(value).is_some())
                        .unwrap_or(default_color);
                    let size = styled
                        .font_size_px
                        .map(|size| size as f32)
                        .unwrap_or(default_size);

                    content.child(
                        div()
                            .text_size(px(size))
                            .text_color(rgb_hex(color))
                            .child(styled.text.clone()),
                    )
                }
                HtmlNode::Image(image) => content.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .py(px(10.0))
                        .bg(rgb_hex(&self.theme.palette.code_background))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(rgb_hex(&self.theme.palette.panel_border))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb_hex(&self.theme.palette.accent))
                                .child("IMG"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .text_color(rgb_hex(&self.theme.palette.text_primary))
                                        .child(
                                            image
                                                .alt
                                                .clone()
                                                .or_else(|| image.src.clone())
                                                .unwrap_or_else(|| "inline image".to_string()),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgb_hex(&self.theme.palette.text_muted))
                                        .child(format!(
                                            "{}{}",
                                            image
                                                .width_px
                                                .map(|width| format!("width {}px", width))
                                                .unwrap_or_else(|| "width auto".to_string()),
                                            image
                                                .src
                                                .as_ref()
                                                .map(|src| format!(" · {}", src))
                                                .unwrap_or_default()
                                        )),
                                ),
                        ),
                ),
            },
        )
    }

    fn render_highlighted_text(
        &self,
        text: &str,
        default_size: f32,
        cursor: Option<&CursorState>,
        cx: Option<&Context<Self>>,
    ) -> Div {
        let spans = self.highlighter.highlight(text);
        let syntax = &self.theme.syntax;
        let palette = &self.theme.palette;

        let mut current_offset = 0;
        let mut content = div().flex().flex_wrap().items_center().gap(px(0.0));

        for span in spans {
            let start = current_offset;
            let end = current_offset + span.text.len();
            current_offset = end;

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

            if let Some(cursor) = cursor {
                let head = cursor.head;
                let anchor = cursor.anchor;
                let sel_start = anchor.map(|a| a.min(head));
                let sel_end = anchor.map(|a| a.max(head));

                let mut split_points = Vec::new();
                if head >= start && head <= end {
                    split_points.push(head);
                }
                if let Some(s) = sel_start {
                    if s >= start && s <= end {
                        split_points.push(s);
                    }
                }
                if let Some(e) = sel_end {
                    if e >= start && e <= end {
                        split_points.push(e);
                    }
                }
                split_points.sort();
                split_points.dedup();

                let mut last_p = start;
                for p in split_points {
                    if p > last_p {
                        let sub_text = &span.text[last_p - start..p - start];
                        content = content.child(self.render_span_fragment(
                            sub_text,
                            default_size,
                            color,
                            last_p,
                            p,
                            sel_start,
                            sel_end,
                            cx,
                        ));
                    }
                    if p == head {
                        content = content.child(
                            div()
                                .w(px(2.0))
                                .h(px(default_size + 2.0))
                                .bg(rgb_hex(&palette.cursor)),
                        );
                    }
                    last_p = p;
                }
                if last_p < end {
                    let sub_text = &span.text[last_p - start..end - start];
                    content = content.child(self.render_span_fragment(
                        sub_text,
                        default_size,
                        color,
                        last_p,
                        end,
                        sel_start,
                        sel_end,
                        cx,
                    ));
                }
            } else {
                content = content.child(
                    div()
                        .text_size(px(default_size))
                        .text_color(rgb_hex(color))
                        .child(span.text.clone()),
                );
            }
        }

        // If cursor is at the very end of the text
        if let Some(cursor) = cursor {
            if cursor.head == text.len() && !text.ends_with('\n') {
                // If the loop didn't render the cursor because it's exactly at text.len()
                // and we didn't have a span ending exactly there that triggered the p == head check
                // or if the text is empty.
                // Wait, if text is empty, the loop won't run.
            }
        }

        // Actually, let's just check if we ever rendered the cursor.
        // Or simpler: if head == text.len(), append cursor at the end.
        if let Some(cursor) = cursor {
            if cursor.head == text.len() {
                content = content.child(
                    div()
                        .w(px(2.0))
                        .h(px(default_size + 2.0))
                        .bg(rgb_hex(&palette.cursor)),
                );
            }
        }

        content
    }

    fn render_span_fragment(
        &self,
        text: &str,
        size: f32,
        color: &str,
        start: usize,
        end: usize,
        sel_start: Option<usize>,
        sel_end: Option<usize>,
        cx: Option<&Context<Self>>,
    ) -> Div {
        let is_selected = if let (Some(s), Some(e)) = (sel_start, sel_end) {
            start >= s && end <= e
        } else {
            false
        };

        if let Some(cx) = cx {
            return clickable_chars(text, start).into_iter().fold(
                div().flex().items_center().gap(px(0.0)),
                |fragment, (offset, ch)| {
                    let mut cell = div()
                        .text_size(px(size))
                        .text_color(rgb_hex(color))
                        .child(ch.clone())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                                let changed = this
                                    .document
                                    .set_focused_cursor(offset, event.modifiers.shift);
                                cx.stop_propagation();
                                if changed {
                                    cx.notify();
                                }
                            }),
                        );

                    if is_selected {
                        cell = cell.bg(rgb_hex(&self.theme.palette.selection));
                    }

                    fragment.child(cell)
                },
            );
        }

        let mut fragment = div()
            .text_size(px(size))
            .text_color(rgb_hex(color))
            .child(text.to_string());

        if is_selected {
            fragment = fragment.bg(rgb_hex(&self.theme.palette.selection));
        }
        fragment
    }
}

impl SolaRoot {
    fn trigger_typst_renders(&mut self, cx: &mut Context<Self>) {
        let requests = self
            .document
            .blocks()
            .iter()
            .enumerate()
            .filter_map(|(index, block)| {
                let Some(TypstAdapter::Pending) = block.typst.as_ref() else {
                    return None;
                };

                typst_render_request(block).map(|(kind, source)| {
                    let cache_key = typst_cache_key(&kind, &source);
                    (index, block.source.clone(), kind, source, cache_key)
                })
            })
            .collect::<Vec<_>>();

        for (index, block_source, kind, source, cache_key) in requests {
            if let Some(cached) = self.typst_cache.get(&cache_key).cloned() {
                if apply_cached_typst_adapter(&mut self.document, &cache_key, cached) > 0 {
                    cx.notify();
                }
                continue;
            }

            if !should_start_typst_compile(&self.typst_cache, &self.typst_in_flight, &cache_key) {
                continue;
            }

            self.typst_in_flight.insert(cache_key.clone());
            cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let background = cx.background_executor().clone();
                let mut async_cx = cx.clone();

                async move {
                    let result = background
                        .spawn(async move { compile_to_svg(&source, kind) })
                        .await;
                    let next_adapter = typst_adapter_from_result(result);

                    let _ = this.update(&mut async_cx, |this, cx| {
                        this.typst_in_flight.remove(&cache_key);
                        this.typst_cache
                            .insert(cache_key.clone(), next_adapter.clone());

                        if apply_completed_typst_work(
                            &mut this.document,
                            &cache_key,
                            index,
                            &block_source,
                            next_adapter.clone(),
                        ) > 0
                        {
                            cx.notify();
                        }
                    });
                }
            })
            .detach();
        }
    }

    fn handle_focused_key_down(&mut self, event: &gpui::KeyDownEvent) -> bool {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;
        let primary = modifiers.control || modifiers.platform;

        if primary && key.eq_ignore_ascii_case("t") {
            self.toggle_theme();
            return true;
        }

        if primary && modifiers.shift && key.eq_ignore_ascii_case("z") {
            return self.document.redo();
        }

        if primary && key.eq_ignore_ascii_case("y") {
            return self.document.redo();
        }

        if primary && key.eq_ignore_ascii_case("z") {
            return self.document.undo();
        }

        if modifiers.alt && key.eq_ignore_ascii_case("up") {
            if self.document.focused_has_draft() {
                self.document.apply_focused_draft();
            }
            return self.document.focus_previous();
        }

        if modifiers.alt && key.eq_ignore_ascii_case("down") {
            if self.document.focused_has_draft() {
                self.document.apply_focused_draft();
            }
            return self.document.focus_next();
        }

        if key.eq_ignore_ascii_case("left") {
            return self.document.move_cursor_left(modifiers.shift);
        }

        if key.eq_ignore_ascii_case("right") {
            return self.document.move_cursor_right(modifiers.shift);
        }

        if key.eq_ignore_ascii_case("up") {
            return self.document.move_cursor_up(modifiers.shift);
        }

        if key.eq_ignore_ascii_case("down") {
            return self.document.move_cursor_down(modifiers.shift);
        }

        if primary && key.eq_ignore_ascii_case("a") {
            return self.document.select_all();
        }

        if primary && key.eq_ignore_ascii_case("n") {
            return self.document.insert_paragraph_after_focused(
                "Inserted via keyboard shortcut as a structure-editing prototype.",
            );
        }

        if primary && key.eq_ignore_ascii_case("d") {
            return self.document.duplicate_focused_block();
        }

        if primary && key.eq_ignore_ascii_case("backspace") {
            return self.document.delete_focused_block();
        }

        if primary && key.eq_ignore_ascii_case("s") {
            return self.document.apply_focused_draft();
        }

        if key.eq_ignore_ascii_case("escape") {
            return self.document.revert_focused_draft();
        }

        if key.eq_ignore_ascii_case("backspace") {
            return self.document.delete_at_cursor_in_focused_draft();
        }

        if key.eq_ignore_ascii_case("enter") {
            return self.document.push_char_to_focused_draft('\n');
        }

        if !modifiers.control && !modifiers.alt && !modifiers.platform {
            if let Some(ch) = event.keystroke.key_char.as_deref() {
                let mut chars = ch.chars();
                if let Some(single) = chars.next() {
                    if chars.next().is_none() {
                        return self.document.push_char_to_focused_draft(single);
                    }
                }
            }
        }

        false
    }
}

impl Render for SolaRoot {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb_hex(&self.theme.palette.app_background))
            .text_color(rgb_hex(&self.theme.palette.text_primary))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(self.render_header(cx))
                    .child(
                        div()
                            .size_full()
                            .flex()
                            .child(self.render_sidebar())
                            .child(self.render_document_surface(cx)),
                    ),
            )
    }
}

impl ThemeMode {
    fn toggle(self) -> Self {
        match self {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }
}

fn rgb_hex(hex: &str) -> Hsla {
    rgb(parse_hex_color(hex).unwrap_or(0xffffff)).into()
}

fn section_title(title: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(14.0))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb_hex(&theme.palette.text_primary))
        .child(title.to_string())
}

fn meta_line(label: &str, value: String, theme: &Theme) -> Div {
    div()
        .flex()
        .justify_between()
        .items_center()
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgb_hex(&theme.palette.text_muted))
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgb_hex(&theme.palette.text_primary))
                .child(value),
        )
}

fn pill(label: &str, value: String, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(8.0))
        .bg(rgb_hex(&theme.palette.panel_background))
        .rounded(px(999.0))
        .border_1()
        .border_color(rgb_hex(&theme.palette.panel_border))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb_hex(&theme.palette.text_muted))
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb_hex(&theme.palette.text_primary))
                .child(value),
        )
}

fn action_button(label: String, theme: &Theme, active: bool) -> Div {
    let border = if active {
        rgb_hex(&theme.palette.focused_border)
    } else {
        rgb_hex(&theme.palette.panel_border)
    };

    let text = if active {
        rgb_hex(&theme.palette.text_primary)
    } else {
        rgb_hex(&theme.palette.text_muted)
    };

    div()
        .flex()
        .items_center()
        .px(px(12.0))
        .py(px(8.0))
        .bg(rgb_hex(&theme.palette.panel_background))
        .rounded(px(999.0))
        .border_1()
        .border_color(border)
        .cursor_pointer()
        .text_size(px(12.0))
        .text_color(text)
        .child(label)
}

fn shortcut_legend(theme: &Theme) -> Div {
    div()
        .flex()
        .flex_wrap()
        .gap(px(8.0))
        .child(shortcut_chip("Ctrl/Cmd+T", "toggle theme", theme))
        .child(shortcut_chip("Ctrl/Cmd+Z", "undo", theme))
        .child(shortcut_chip("Ctrl/Cmd+Shift+Z", "redo", theme))
        .child(shortcut_chip("Alt+↑/↓", "move focus", theme))
        .child(shortcut_chip("←/→", "move cursor", theme))
        .child(shortcut_chip("↑/↓", "move line", theme))
        .child(shortcut_chip("Shift+←/→", "select", theme))
        .child(shortcut_chip("Shift+↑/↓", "select line", theme))
        .child(shortcut_chip("Ctrl/Cmd+A", "select all", theme))
        .child(shortcut_chip("Ctrl/Cmd+N", "insert paragraph", theme))
        .child(shortcut_chip("Ctrl/Cmd+D", "duplicate block", theme))
        .child(shortcut_chip("Ctrl/Cmd+Backspace", "delete block", theme))
        .child(shortcut_chip("Ctrl/Cmd+S", "apply draft", theme))
        .child(shortcut_chip("Esc", "revert draft", theme))
        .child(shortcut_chip("Backspace", "edit draft", theme))
        .child(shortcut_chip("Enter", "newline in draft", theme))
}

fn shortcut_chip(key: &str, label: &str, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(10.0))
        .py(px(6.0))
        .bg(rgb_hex(&theme.palette.panel_background))
        .rounded(px(999.0))
        .border_1()
        .border_color(rgb_hex(&theme.palette.panel_border))
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb_hex(&theme.palette.text_primary))
                .child(key.to_string()),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb_hex(&theme.palette.text_muted))
                .child(label.to_string()),
        )
}

fn truncate_for_pill(input: &str, max_chars: usize) -> String {
    let mut output = input.chars().take(max_chars).collect::<String>();
    if input.chars().count() > max_chars {
        output.push('…');
    }
    output
}

fn typst_render_request(block: &DocumentBlock) -> Option<(RenderKind, String)> {
    match block.kind {
        BlockKind::MathBlock => Some((RenderKind::Math, block.rendered.clone())),
        BlockKind::TypstBlock => Some((RenderKind::Block, block.rendered.clone())),
        BlockKind::Paragraph | BlockKind::ListItem { .. } | BlockKind::Quote
            if block.typst.is_some() =>
        {
            Some((RenderKind::Block, block.rendered.clone()))
        }
        BlockKind::Heading { .. }
        | BlockKind::Paragraph
        | BlockKind::ListItem { .. }
        | BlockKind::Quote
        | BlockKind::CodeFence { .. } => None,
    }
}

#[cfg(test)]
fn apply_typst_result(block: &mut DocumentBlock, result: Result<String, TypstError>) -> bool {
    apply_typst_adapter(block, typst_adapter_from_result(result))
}

fn apply_typst_adapter(block: &mut DocumentBlock, adapter: TypstAdapter) -> bool {
    if block.typst.is_none() {
        return false;
    }

    block.typst = Some(adapter);

    true
}

fn clickable_chars(text: &str, start: usize) -> Vec<(usize, String)> {
    text.char_indices()
        .map(|(offset, ch)| (start + offset, ch.to_string()))
        .collect()
}

fn typst_cache_key(kind: &RenderKind, source: &str) -> String {
    let prefix = match kind {
        RenderKind::Math => "math",
        RenderKind::Block => "block",
    };
    format!("{prefix}::{source}")
}

fn typst_adapter_from_result(result: Result<String, TypstError>) -> TypstAdapter {
    match result {
        Ok(svg) => TypstAdapter::Rendered { svg },
        Err(error) => TypstAdapter::Error {
            message: error.to_string(),
        },
    }
}

fn should_start_typst_compile(
    cache: &HashMap<String, TypstAdapter>,
    in_flight: &HashSet<String>,
    cache_key: &str,
) -> bool {
    !cache.contains_key(cache_key) && !in_flight.contains(cache_key)
}

fn apply_cached_typst_adapter(
    document: &mut DocumentModel,
    cache_key: &str,
    adapter: TypstAdapter,
) -> usize {
    let previous_focus = document.focused_block();
    let mut targets = Vec::new();

    for (index, block) in document.blocks().iter().enumerate() {
        if !matches!(block.typst, Some(TypstAdapter::Pending)) {
            continue;
        }

        let Some((kind, source)) = typst_render_request(block) else {
            continue;
        };

        if typst_cache_key(&kind, &source) == cache_key {
            targets.push(index);
        }
    }

    let mut updated = 0;
    for index in targets {
        if !document.focus_block(index) {
            continue;
        }

        if let Some(block) = document.focused_block_mut()
            && apply_typst_adapter(block, adapter.clone())
        {
            updated += 1;
        }
    }

    let restore_index = previous_focus.min(document.block_count().saturating_sub(1));
    let _ = document.focus_block(restore_index);
    updated
}

fn apply_completed_typst_work(
    document: &mut DocumentModel,
    cache_key: &str,
    _origin_index: usize,
    _origin_source: &str,
    adapter: TypstAdapter,
) -> usize {
    apply_cached_typst_adapter(document, cache_key, adapter)
}

#[cfg(target_os = "linux")]
fn ensure_linux_display_backend() -> Result<(), String> {
    if wayland_socket_reachable() || x11_socket_reachable() {
        return Ok(());
    }

    Err("Sola skipped GPUI startup: no reachable Wayland compositor or X11 display was detected in the current environment.".to_string())
}

#[cfg(target_os = "linux")]
fn wayland_socket_reachable() -> bool {
    let runtime_dir = match env::var_os("XDG_RUNTIME_DIR") {
        Some(value) => PathBuf::from(value),
        None => return false,
    };

    let wayland_display = match env::var_os("WAYLAND_DISPLAY") {
        Some(value) => value,
        None => return false,
    };

    let socket_path = runtime_dir.join(wayland_display);
    unix_socket_reachable(&socket_path)
}

#[cfg(target_os = "linux")]
fn x11_socket_reachable() -> bool {
    let display = match env::var("DISPLAY") {
        Ok(value) => value,
        Err(_) => return false,
    };

    let display_suffix = display
        .trim()
        .trim_start_matches(':')
        .split('.')
        .next()
        .unwrap_or_default();

    if display_suffix.is_empty() || !display_suffix.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }

    let socket_path = PathBuf::from(format!("/tmp/.X11-unix/X{display_suffix}"));
    unix_socket_reachable(&socket_path)
}

#[cfg(target_os = "linux")]
fn unix_socket_reachable(path: &Path) -> bool {
    path.exists() && UnixStream::connect(path).is_ok()
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use super::unix_socket_reachable;
    use super::{
        apply_cached_typst_adapter, apply_completed_typst_work, apply_typst_result,
        clickable_chars, should_start_typst_compile, typst_adapter_from_result, typst_cache_key,
        typst_render_request,
    };
    use sola_document::{DocumentModel, TypstAdapter};
    use sola_typst::{RenderKind, TypstError};
    use std::collections::{HashMap, HashSet};
    #[cfg(target_os = "linux")]
    use std::path::Path;

    #[cfg(target_os = "linux")]
    #[test]
    fn missing_unix_socket_is_reported_as_unreachable() {
        assert!(!unix_socket_reachable(Path::new(
            "/tmp/sola-missing-socket"
        )));
    }

    #[test]
    fn typst_render_request_maps_supported_blocks_to_jobs() {
        let math = DocumentModel::from_markdown("$$a + b$$");
        let typst = DocumentModel::from_markdown(
            r#"```typst
#set text(fill: red)
Hello
```"#,
        );
        let plain = DocumentModel::from_markdown("plain paragraph");

        let Some((math_kind, math_source)) = typst_render_request(&math.blocks()[0]) else {
            panic!("expected math render request");
        };
        assert!(matches!(math_kind, RenderKind::Math));
        assert_eq!(math_source, "a + b");

        let Some((typst_kind, typst_source)) = typst_render_request(&typst.blocks()[0]) else {
            panic!("expected typst render request");
        };
        assert!(matches!(typst_kind, RenderKind::Block));
        assert_eq!(typst_source, "#set text(fill: red)\nHello");

        assert!(typst_render_request(&plain.blocks()[0]).is_none());
    }

    #[test]
    fn apply_typst_result_updates_block_render_state() {
        let mut document = DocumentModel::from_markdown("$$a + b$$");
        let block = document.focused_block_mut().unwrap();

        assert!(apply_typst_result(block, Ok("<svg />".to_string())));
        assert!(matches!(
            block.typst,
            Some(TypstAdapter::Rendered { ref svg }) if svg == "<svg />"
        ));

        assert!(apply_typst_result(
            block,
            Err(TypstError::Compile("bad typst".to_string()))
        ));
        assert!(matches!(
            block.typst,
            Some(TypstAdapter::Error { ref message }) if message.contains("bad typst")
        ));
    }

    #[test]
    fn typst_render_request_maps_inline_math_paragraphs_to_block_jobs() {
        let document = DocumentModel::from_markdown("Paragraph with $a + b$ inline math.");

        let Some((kind, source)) = typst_render_request(&document.blocks()[0]) else {
            panic!("expected inline math render request");
        };

        assert!(matches!(kind, RenderKind::Block));
        assert_eq!(source, "Paragraph with $a + b$ inline math.");
    }

    #[test]
    fn clickable_chars_preserve_utf8_offsets() {
        let chars = clickable_chars("a好b", 10);

        assert_eq!(
            chars,
            vec![
                (10, "a".to_string()),
                (11, "好".to_string()),
                (14, "b".to_string())
            ]
        );
    }

    #[test]
    fn typst_cache_key_distinguishes_render_kind() {
        assert_ne!(
            typst_cache_key(&RenderKind::Math, "x + y"),
            typst_cache_key(&RenderKind::Block, "x + y")
        );
        assert_eq!(
            typst_cache_key(&RenderKind::Math, "x + y"),
            typst_cache_key(&RenderKind::Math, "x + y")
        );
    }

    #[test]
    fn typst_adapter_from_result_maps_success_and_error() {
        assert!(matches!(
            typst_adapter_from_result(Ok("<svg />".to_string())),
            TypstAdapter::Rendered { ref svg } if svg == "<svg />"
        ));
        assert!(matches!(
            typst_adapter_from_result(Err(TypstError::Compile("bad typst".to_string()))),
            TypstAdapter::Error { ref message } if message.contains("bad typst")
        ));
    }

    #[test]
    fn should_start_typst_compile_skips_cached_and_inflight_keys() {
        let key = typst_cache_key(&RenderKind::Math, "x + y");
        let mut cache = HashMap::new();
        let mut in_flight = HashSet::new();

        assert!(should_start_typst_compile(&cache, &in_flight, &key));

        cache.insert(
            key.clone(),
            TypstAdapter::Rendered {
                svg: "<svg />".into(),
            },
        );
        assert!(!should_start_typst_compile(&cache, &in_flight, &key));

        cache.clear();
        in_flight.insert(key.clone());
        assert!(!should_start_typst_compile(&cache, &in_flight, &key));
    }

    #[test]
    fn apply_cached_typst_adapter_updates_all_matching_pending_blocks() {
        let mut document = DocumentModel::from_markdown(
            r#"$$a + b$$

$$a + b$$

$$c + d$$"#,
        );

        let key = typst_cache_key(&RenderKind::Math, "a + b");
        let updated = apply_cached_typst_adapter(
            &mut document,
            &key,
            TypstAdapter::Rendered {
                svg: "<svg>stable</svg>".to_string(),
            },
        );

        assert_eq!(updated, 2);
        assert!(matches!(
            document.blocks()[0].typst,
            Some(TypstAdapter::Rendered { ref svg }) if svg == "<svg>stable</svg>"
        ));
        assert!(matches!(
            document.blocks()[1].typst,
            Some(TypstAdapter::Rendered { ref svg }) if svg == "<svg>stable</svg>"
        ));
        assert!(matches!(
            document.blocks()[2].typst,
            Some(TypstAdapter::Pending)
        ));
    }

    #[test]
    fn apply_completed_typst_work_still_updates_matching_peers_when_origin_changed() {
        let mut document = DocumentModel::from_markdown(
            r#"$$a + b$$

$$a + b$$"#,
        );

        document.focus_block(0);
        document.focused_block_mut().unwrap().source = "$$changed$$".to_string();
        document.focused_block_mut().unwrap().rendered = "changed".to_string();
        document.focused_block_mut().unwrap().typst = Some(TypstAdapter::Pending);

        let updated = apply_completed_typst_work(
            &mut document,
            &typst_cache_key(&RenderKind::Math, "a + b"),
            0,
            "$$a + b$$",
            TypstAdapter::Rendered {
                svg: "<svg>stable</svg>".to_string(),
            },
        );

        assert_eq!(updated, 1);
        assert!(matches!(
            document.blocks()[0].typst,
            Some(TypstAdapter::Pending)
        ));
        assert!(matches!(
            document.blocks()[1].typst,
            Some(TypstAdapter::Rendered { ref svg }) if svg == "<svg>stable</svg>"
        ));
    }
}
