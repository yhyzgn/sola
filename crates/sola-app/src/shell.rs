use gpui::{
    AppContext, Application, Bounds, Context, Div, FocusHandle, FontWeight, Hsla,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Window, WindowBounds,
    WindowOptions, div, px, rgb, size,
};
use sola_core::{APP_NAME, APP_TAGLINE, ROADMAP_PHASES, sample_markdown};
use sola_document::{BlockKind, DocumentBlock, DocumentModel};
use sola_theme::{Theme, parse_hex_color};
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
        let mut toggle_theme = action_button(
            format!("theme: {}", self.theme_mode.label()),
            &self.theme,
            true,
        );
        toggle_theme
            .interactivity()
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

    fn render_document_surface(&self, cx: &mut Context<Self>) -> Div {
        let blocks = self.document.blocks().iter().enumerate().fold(
            div().flex().flex_col().gap(px(14.0)).p(px(24.0)),
            |surface, (index, block)| surface.child(self.render_block(index, block, cx)),
        );

        let mut previous_button = action_button(
            "← previous block".to_string(),
            &self.theme,
            self.document.focused_block() > 0,
        );
        previous_button
            .interactivity()
            .on_click(cx.listener(|this, _event, _window, cx| {
                if this.document.focus_previous() {
                    cx.notify();
                }
            }));

        let mut next_button = action_button(
            "next block →".to_string(),
            &self.theme,
            self.document.focused_block() + 1 < self.document.block_count(),
        );
        next_button
            .interactivity()
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
        let mut insert_button = action_button("insert paragraph".to_string(), &self.theme, true);
        insert_button
            .interactivity()
            .on_click(cx.listener(|this, _event, _window, cx| {
                if this.document.insert_paragraph_after_focused(
                    "A new paragraph block inserted by the structure editing prototype.",
                ) {
                    cx.notify();
                }
            }));

        let mut duplicate_button = action_button("duplicate block".to_string(), &self.theme, true);
        duplicate_button
            .interactivity()
            .on_click(cx.listener(|this, _event, _window, cx| {
                if this.document.duplicate_focused_block() {
                    cx.notify();
                }
            }));

        let mut delete_button = action_button(
            "delete block".to_string(),
            &self.theme,
            self.document.block_count() > 1,
        );
        delete_button
            .interactivity()
            .on_click(cx.listener(|this, _event, _window, cx| {
                if this.document.delete_focused_block() {
                    cx.notify();
                }
            }));

        div()
            .flex()
            .flex_col()
            .size_full()
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
            .child(blocks)
    }

    fn render_block(&self, index: usize, block: &DocumentBlock, cx: &mut Context<Self>) -> Div {
        let is_focused = self.document.focused_block() == index;
        let border = if is_focused {
            rgb_hex(&self.theme.palette.focused_border)
        } else {
            rgb_hex(&self.theme.palette.panel_border)
        };
        let background = if is_focused {
            rgb_hex(&self.theme.palette.focused_background)
        } else {
            rgb_hex(&self.theme.palette.panel_background)
        };

        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(16.0))
            .bg(background)
            .rounded(px(14.0))
            .border_1()
            .border_color(border)
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(pill(
                        block.kind.label(),
                        if is_focused {
                            "focused".to_string()
                        } else {
                            "blurred".to_string()
                        },
                        &self.theme,
                    ))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb_hex(&self.theme.palette.text_muted))
                            .child(format!("block #{:02}", index + 1)),
                    ),
            );

        if is_focused {
            let mut append_button =
                action_button("append draft note".to_string(), &self.theme, true);
            append_button
                .interactivity()
                .on_click(cx.listener(|this, _event, _window, cx| {
                    if this
                        .document
                        .append_to_focused_draft("\nEdited in the focused source prototype.")
                    {
                        cx.notify();
                    }
                }));

            let mut revert_button = action_button(
                "revert draft".to_string(),
                &self.theme,
                self.document.focused_has_draft(),
            );
            revert_button
                .interactivity()
                .on_click(cx.listener(|this, _event, _window, cx| {
                    if this.document.revert_focused_draft() {
                        cx.notify();
                    }
                }));

            let mut apply_button = action_button(
                "apply draft".to_string(),
                &self.theme,
                self.document.focused_has_draft(),
            );
            apply_button
                .interactivity()
                .on_click(cx.listener(|this, _event, _window, cx| {
                    if this.document.apply_focused_draft() {
                        cx.notify();
                    }
                }));

            card = card
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(rgb_hex(&self.theme.palette.text_muted))
                        .child("Focused state · editable markdown source draft"),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(10.0))
                        .items_center()
                        .child(append_button)
                        .child(revert_button)
                        .child(apply_button),
                )
                .child(
                    div()
                        .p(px(14.0))
                        .bg(rgb_hex(&self.theme.palette.code_background))
                        .rounded(px(10.0))
                        .child(
                            self.document
                                .focused_text()
                                .unwrap_or(&block.source)
                                .to_string(),
                        ),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(rgb_hex(&self.theme.palette.text_muted))
                        .child("Blurred preview"),
                )
                .child(self.render_blurred_content(block));
        } else {
            card = card.track_focus(&self.focus_handle);
            card = card.child(self.render_blurred_content(block));
        }

        card = card.on_key_down(cx.listener(|this, event, _window, cx| {
            if this.handle_focused_key_down(event) {
                cx.notify();
            }
        }));

        card.interactivity()
            .on_click(cx.listener(move |this, _event, window, cx| {
                if this.document.focus_block(index) {
                    window.focus(&this.focus_handle);
                    cx.notify();
                }
            }));

        card
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
            BlockKind::Paragraph => div()
                .text_size(px(self.theme.typography.body_size as f32))
                .text_color(rgb_hex(&self.theme.palette.text_primary))
                .child(block.rendered.clone()),
            BlockKind::ListItem { ordered } => div()
                .flex()
                .gap(px(10.0))
                .child(
                    div()
                        .text_color(rgb_hex(&self.theme.palette.accent))
                        .font_weight(FontWeight::BOLD)
                        .child(if *ordered { "1." } else { "•" }),
                )
                .child(
                    div()
                        .text_color(rgb_hex(&self.theme.palette.text_primary))
                        .child(block.rendered.clone()),
                ),
            BlockKind::Quote => div()
                .pl(px(14.0))
                .border_l_2()
                .border_color(rgb_hex(&self.theme.palette.accent))
                .text_color(rgb_hex(&self.theme.palette.text_muted))
                .child(block.rendered.clone()),
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
                        .child(block.rendered.clone()),
                ),
        }
    }
}

impl SolaRoot {
    fn handle_focused_key_down(&mut self, event: &gpui::KeyDownEvent) -> bool {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;
        let primary = modifiers.control || modifiers.platform;

        if primary && key.eq_ignore_ascii_case("t") {
            self.toggle_theme();
            return true;
        }

        if modifiers.alt && key.eq_ignore_ascii_case("up") {
            return self.document.focus_previous();
        }

        if modifiers.alt && key.eq_ignore_ascii_case("down") {
            return self.document.focus_next();
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
            return self.document.delete_last_char_from_focused_draft();
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
        .child(shortcut_chip("Alt+↑/↓", "move focus", theme))
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
    #[cfg(target_os = "linux")]
    use std::path::Path;

    #[cfg(target_os = "linux")]
    #[test]
    fn missing_unix_socket_is_reported_as_unreachable() {
        assert!(!unix_socket_reachable(Path::new(
            "/tmp/sola-missing-socket"
        )));
    }
}
