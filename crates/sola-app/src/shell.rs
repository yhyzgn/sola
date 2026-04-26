use crate::actions::{Open, Quit, Redo, Save, ToggleTheme, Undo};
use crate::focused_editor::{
    FocusedEditorElement, FocusedEditorStyle, approximate_editor_wrap_width,
    move_cursor_vertical_visual, shape_focused_lines, spans_to_runs, visual_line_edge_offset,
    visual_line_ranges,
};
use crate::project_panel::ProjectPanel;
use crate::workspace::Workspace;
use crate::worktree::Worktree;
use gpui::{
    AppContext, Application, AsyncApp, Bounds, Context, Div, Entity, FocusHandle, FontWeight, Hsla,
    Image, ImageFormat, InteractiveElement, IntoElement, KeyBinding, Menu, MenuItem, ParentElement,
    Render, StatefulInteractiveElement, Styled, WeakEntity, Window, WindowBounds, WindowOptions,
    div, img, px, rgb, size,
};

use sola_core::{APP_NAME, APP_TAGLINE, sample_markdown};
use sola_document::highlighter::SyntaxHighlighter;
use sola_document::{BlockKind, DocumentBlock, DocumentModel, HtmlAdapter, HtmlNode, TypstAdapter};
use sola_theme::{Theme, parse_hex_color};
use sola_typst::{RenderKind, TypstError, compile_to_svg};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
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
        cx.set_menus(vec![
            Menu {
                name: "Sola".into(),
                items: vec![MenuItem::action("Quit", Quit)],
            },
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("Open...", Open),
                    MenuItem::action("Save", Save),
                ],
            },
            Menu {
                name: "Edit".into(),
                items: vec![
                    MenuItem::action("Undo", Undo),
                    MenuItem::action("Redo", Redo),
                ],
            },
            Menu {
                name: "View".into(),
                items: vec![MenuItem::action("Toggle Theme", ToggleTheme)],
            },
        ]);

        cx.bind_keys([
            KeyBinding::new("cmd-o", Open, None),
            KeyBinding::new("ctrl-o", Open, None),
            KeyBinding::new("cmd-s", Save, None),
            KeyBinding::new("ctrl-s", Save, None),
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("ctrl-q", Quit, None),
            KeyBinding::new("cmd-z", Undo, None),
            KeyBinding::new("ctrl-z", Undo, None),
            KeyBinding::new("cmd-shift-z", Redo, None),
            KeyBinding::new("ctrl-shift-z", Redo, None),
            KeyBinding::new("cmd-t", ToggleTheme, None),
            KeyBinding::new("ctrl-t", ToggleTheme, None),
        ]);

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
            |_window, cx| {
                let handle = cx.new(|cx| SolaRoot::new(cx));
                let weak_handle = handle.downgrade();
                handle.update(cx, |this, _| {
                    this.this_handle = Some(weak_handle);
                });
                _window.focus(&handle.read(cx).focus_handle);
                handle
            },
        )
        .expect("open GPUI window");
    });
}

struct SolaRoot {
    focus_handle: FocusHandle,
    this_handle: Option<WeakEntity<Self>>,
    workspace: Entity<Workspace>,
    project_panel: Entity<ProjectPanel>,
    highlighter: SyntaxHighlighter,
    typst_cache: HashMap<String, TypstAdapter>,
    typst_in_flight: HashSet<String>,
    cursor_visible: bool,
    cursor_blink_started: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BlockClickPlan {
    apply_draft: bool,
    switch_block_focus: bool,
    refresh_window_focus: bool,
}

impl SolaRoot {
    fn new(cx: &mut Context<Self>) -> Self {
        let worktree = Worktree::local(".", cx);
        let workspace = cx.new(|cx| Workspace::new(worktree, cx));
        let project_panel = cx.new(|cx| ProjectPanel::new(workspace.clone(), cx));
        let weak_panel = project_panel.downgrade();
        project_panel.update(cx, |panel, cx| {
            panel.set_handle(weak_panel, cx);
        });

        // Sync initial document with sample markdown
        workspace.update(cx, |this, cx| {
            this.update_document(cx, |doc| {
                *doc = DocumentModel::from_markdown(sample_markdown());
            });
        });

        use crate::workspace::WorkspaceEvent;
        cx.subscribe(&workspace, |this, _workspace, event, cx| match event {
            WorkspaceEvent::DocumentChanged => {
                this.trigger_typst_renders(cx);
            }
            WorkspaceEvent::ThemeChanged => {}
            WorkspaceEvent::WorktreeChanged => {} // Not handled here anymore
        })
        .detach();

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            this_handle: None,
            workspace,
            project_panel,
            highlighter: SyntaxHighlighter::new_rust(),
            typst_cache: HashMap::new(),
            typst_in_flight: HashSet::new(),
            cursor_visible: true,
            cursor_blink_started: false,
        };

        this.trigger_typst_renders(cx);
        this
    }

    fn open_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        use gpui::PathPromptOptions;
        let this_handle = self.this_handle.clone();

        let paths_rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some("Open File or Folder".into()),
        });

        cx.spawn(|_this, cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                if let Ok(Ok(Some(paths))) = paths_rx.await {
                    if let Some(path) = paths.first() {
                        if let Some(this_handle) = this_handle {
                            let path = path.clone();
                            let _ = this_handle.update(&mut cx, |this, cx| {
                                if path.is_dir() {
                                    // New Worktree
                                    let worktree = Worktree::local(path, cx);
                                    this.workspace.update(cx, |workspace, cx| {
                                        workspace.update_worktree(worktree, cx);
                                    });
                                } else {
                                    // Open File
                                    let dir = path.parent().unwrap_or(&path);
                                    let worktree = Worktree::local(dir, cx);
                                    this.workspace.update(cx, |workspace, cx| {
                                        workspace.update_worktree(worktree, cx);
                                        workspace.open_file(path, cx);
                                    });
                                }
                                cx.notify();
                            });
                        }
                    }
                }
            }
        })
        .detach();
    }

    fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.toggle_theme(cx);
        });
    }

    fn ensure_cursor_blink_loop(&mut self, cx: &mut Context<Self>) {
        if self.cursor_blink_started {
            return;
        }

        self.cursor_blink_started = true;
        cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let background = cx.background_executor().clone();
            let mut async_cx = cx.clone();

            async move {
                loop {
                    background.timer(Duration::from_millis(530)).await;
                    if this
                        .update(&mut async_cx, |this, cx| {
                            this.cursor_visible = !this.cursor_visible;
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
        })
        .detach();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> Div {
        let workspace = self.workspace.read(cx);
        let theme = workspace.theme();

        let open_btn = action_button("Open...".to_string(), theme, true)
            .id("open-project")
            .on_click(cx.listener(|this, _event, window, cx| {
                this.open_project(window, cx);
            }));

        let save_btn = action_button("Save".to_string(), theme, true)
            .id("save-project")
            .on_click(cx.listener(|this, _event, _window, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.save_current_file(cx);
                });
            }));

        let toggle_theme = action_button(
            format!("theme: {}", workspace.theme_mode().label()),
            theme,
            true,
        )
        .id("toggle-theme")
        .on_click(cx.listener(|this, _event, _window, cx| {
            this.toggle_theme(cx);
        }));

        div()
            .flex()
            .justify_between()
            .items_center()
            .p(px(20.0))
            .border_b_1()
            .border_color(rgb_hex(&theme.palette.panel_border))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(theme.typography.title_size as f32))
                            .font_weight(FontWeight::BOLD)
                            .child(APP_NAME),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(rgb_hex(&theme.palette.text_muted))
                            .child(APP_TAGLINE),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(open_btn)
                    .child(save_btn)
                    .child(toggle_theme)
                    .child(pill("workspace", format!("{} crates", 4), theme))
                    .child(pill(
                        "focused block",
                        format!("#{}", workspace.document().focused_block() + 1),
                        theme,
                    ))
                    .child(pill("roadmap", "phase 1 / 2 / 5".to_string(), theme)),
            )
    }

    fn render_document_surface(&mut self, cx: &mut Context<Self>) -> Div {
        let (theme, document) = {
            let workspace = self.workspace.read(cx);
            (workspace.theme().clone(), workspace.document().clone())
        };

        self.ensure_cursor_blink_loop(cx);
        let blocks = document.blocks().iter().enumerate().fold(
            div().flex().flex_col().gap(px(14.0)).p(px(24.0)),
            |surface, (index, block)| surface.child(self.render_block(index, block, cx)),
        );

        let previous_button = {
            let workspace = self.workspace.clone();
            action_button(
                "← previous block".to_string(),
                &theme,
                document.focused_block() > 0,
            )
            .id("previous-block")
            .on_click(cx.listener(move |_this, _event, _window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.update_document(cx, |document| {
                        document.focus_previous();
                    });
                });
            }))
        };

        let next_button = {
            let workspace = self.workspace.clone();
            action_button(
                "next block →".to_string(),
                &theme,
                document.focused_block() + 1 < document.block_count(),
            )
            .id("next-block")
            .on_click(cx.listener(move |_this, _event, _window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.update_document(cx, |document| {
                        document.focus_next();
                    });
                });
            }))
        };

        let focused_summary = document
            .focused_block_ref()
            .map(|block| block.rendered.clone())
            .unwrap_or_else(|| "no focused block".to_string());
        let draft_label = if document.focused_has_draft() {
            "draft pending"
        } else {
            "source synced"
        };
        let insert_button = {
            let workspace = self.workspace.clone();
            action_button("insert paragraph".to_string(), &theme, true)
                .id("insert-paragraph")
                .on_click(cx.listener(move |_this, _event, _window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.update_document(cx, |document| {
                            document.insert_paragraph_after_focused(
                                "A new paragraph block inserted by the structure editing prototype.",
                            );
                        });
                    });
                }))
        };

        let duplicate_button = {
            let workspace = self.workspace.clone();
            action_button("duplicate block".to_string(), &theme, true)
                .id("duplicate-block")
                .on_click(cx.listener(move |_this, _event, _window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.update_document(cx, |document| {
                            document.duplicate_focused_block();
                        });
                    });
                }))
        };

        let delete_button = {
            let workspace = self.workspace.clone();
            action_button(
                "delete block".to_string(),
                &theme,
                document.block_count() > 1,
            )
            .id("delete-block")
            .on_click(cx.listener(move |_this, _event, _window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.update_document(cx, |document| {
                        document.delete_focused_block();
                    });
                });
            }))
        };

        let undo_button = {
            let workspace = self.workspace.clone();
            action_button("undo".to_string(), &theme, document.can_undo())
                .id("undo")
                .on_click(cx.listener(move |_this, _event, _window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.update_document(cx, |document| {
                            document.undo();
                        });
                    });
                }))
        };

        let redo_button = {
            let workspace = self.workspace.clone();
            action_button("redo".to_string(), &theme, document.can_redo())
                .id("redo")
                .on_click(cx.listener(move |_this, _event, _window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.update_document(cx, |document| {
                            document.redo();
                        });
                    });
                }))
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event, window, cx| {
                if this.handle_focused_key_down(event, window, cx) {
                    cx.notify();
                }
            }))
            .child(
                div()
                    .p(px(24.0))
                    .border_b_1()
                    .border_color(rgb_hex(&theme.palette.panel_border))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(section_title(
                                "Dual-state engine prototype",
                                &theme,
                            ))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb_hex(&theme.palette.text_muted))
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
                                        &theme,
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
                                &theme,
                            ))
                            .child(shortcut_chip("Ctrl/Cmd+T", "toggle theme", &theme))
                            .child(shortcut_legend(&theme)),
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
        let workspace = self.workspace.read(cx);
        let theme = workspace.theme();
        let document = workspace.document();
        let is_focused = document.focused_block() == index;

        let block_container = div()
            .id(("block-container", index))
            .flex()
            .flex_row()
            .gap(px(12.0))
            .p(px(8.0))
            .cursor_pointer();

        // Subtle focused indicator (accent color line on the left)
        let indicator = if is_focused {
            div()
                .w(px(2.0))
                .bg(rgb_hex(&theme.palette.accent))
                .rounded_full()
        } else {
            div().w(px(2.0))
        };

        let content = if is_focused {
            let editor_style = FocusedEditorStyle::from_theme(theme);
            let text = document.focused_text().unwrap_or(&block.source).to_string();
            let spans = self.highlighter.highlight(&text);
            let runs = spans_to_runs(&spans, &editor_style, theme);
            let selection_color = rgb_hex(&theme.palette.selection);
            let cursor_color = rgb_hex(&theme.palette.cursor);

            if let Some(this_handle) = self.this_handle.clone() {
                div().flex_1().child(
                    div()
                        .bg(rgb_hex(&theme.palette.code_background))
                        .rounded(px(8.0))
                        .child(
                            FocusedEditorElement::new(
                                text,
                                editor_style,
                                runs,
                                document.focused_cursor().cloned(),
                                self.cursor_visible,
                                selection_color,
                                cursor_color,
                            )
                            .on_cursor_move(move |offset, shift, window, cx| {
                                let _ = this_handle.update(cx, |this, cx| {
                                    this.workspace.update(cx, |workspace, cx| {
                                        window.focus(&this.focus_handle);
                                        this.cursor_visible = true;
                                        workspace.update_document(cx, |document| {
                                            document.set_focused_cursor(offset, shift);
                                        });
                                    });
                                });
                            }),
                        ),
                )
            } else {
                div().flex_1()
            }
        } else {
            div().flex_1().child(self.render_blurred_content(block, theme))
        };

        block_container
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.workspace.update(cx, |workspace, cx| {
                        workspace.update_document(cx, |document| {
                            let plan = plan_block_click(
                                document.focused_block(),
                                index,
                                document.focused_has_draft(),
                            );

                            if plan.apply_draft {
                                document.apply_focused_draft();
                            }

                            if plan.switch_block_focus {
                                document.focus_block(index);
                            }

                            if plan.refresh_window_focus {
                                window.focus(&this.focus_handle);
                            }
                        });
                    });
                }),
            )
            .child(indicator)
            .child(content)
    }

    fn render_blurred_content(&self, block: &DocumentBlock, theme: &Theme) -> Div {
        match &block.kind {
            BlockKind::Heading { level } => div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb_hex(&theme.palette.accent))
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
                    self.render_typst_preview(block, "Paragraph", theme)
                } else {
                    self.render_textual_block(
                        block,
                        theme.typography.body_size as f32,
                        &theme.palette.text_primary,
                        theme,
                    )
                }
            }
            BlockKind::ListItem { ordered } => div()
                .flex()
                .gap(px(10.0))
                .child(
                    div()
                        .text_color(rgb_hex(&theme.palette.accent))
                        .font_weight(FontWeight::BOLD)
                        .child(if *ordered { "1." } else { "•" }),
                )
                .child(if block.typst.is_some() {
                    self.render_typst_preview(block, "List item", theme)
                } else {
                    self.render_textual_block(
                        block,
                        theme.typography.body_size as f32,
                        &theme.palette.text_primary,
                        theme,
                    )
                }),
            BlockKind::Quote => div()
                .pl(px(14.0))
                .border_l_2()
                .border_color(rgb_hex(&theme.palette.accent))
                .child(if block.typst.is_some() {
                    self.render_typst_preview(block, "Quote", theme)
                } else {
                    self.render_textual_block(
                        block,
                        theme.typography.body_size as f32,
                        &theme.palette.text_muted,
                        theme,
                    )
                }),
            BlockKind::CodeFence { language } => {
                let editor_style = FocusedEditorStyle::from_theme(theme);
                let spans = self.highlighter.highlight(&block.rendered);
                let runs = spans_to_runs(&spans, &editor_style, theme);

                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb_hex(&theme.palette.text_muted))
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
                            .id(("code-block-scroll", block.id))
                            .p(px(14.0))
                            .bg(rgb_hex(&theme.palette.code_background))
                            .rounded(px(10.0))
                            .overflow_x_scroll()
                            .child(FocusedEditorElement::new(
                                &block.rendered,
                                editor_style,
                                runs,
                                None,
                                false,
                                rgb_hex(&theme.palette.selection),
                                rgb_hex(&theme.palette.cursor),
                            )),
                    )
            }
            BlockKind::MathBlock => self.render_typst_preview(block, "Math block", theme),
            BlockKind::TypstBlock => self.render_typst_preview(block, "Typst block", theme),
        }
    }

    fn render_typst_preview(&self, block: &DocumentBlock, label: &str, theme: &Theme) -> Div {
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
                        .text_color(rgb_hex(&theme.palette.text_muted))
                        .child(format!("{label} · rendering")),
                )
                .child(
                    div()
                        .p(px(14.0))
                        .bg(rgb_hex(&theme.palette.code_background))
                        .rounded(px(10.0))
                        .text_size(px(13.0))
                        .text_color(rgb_hex(&theme.palette.text_muted))
                        .child("Rendering Typst preview..."),
                ),
            Some(TypstAdapter::Rendered { svg }) => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb_hex(&theme.palette.text_muted))
                        .child(format!("{label} · rendered")),
                )
                .child(
                    div()
                        .p(px(14.0))
                        .bg(rgb_hex(&theme.palette.code_background))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(rgb_hex(&theme.palette.panel_border))
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
                        .text_color(rgb_hex(&theme.palette.text_muted))
                        .child(format!("{label} · error")),
                )
                .child(
                    div()
                        .p(px(14.0))
                        .bg(rgb_hex(&theme.palette.code_background))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(rgb_hex(&theme.palette.panel_border))
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
                                .text_color(rgb_hex(&theme.palette.text_muted))
                                .child(block.rendered.clone()),
                        ),
                ),
            None => div()
                .text_size(px(theme.typography.body_size as f32))
                .text_color(rgb_hex(&theme.palette.text_primary))
                .child(block.rendered.clone()),
        }
    }

    fn render_textual_block(&self, block: &DocumentBlock, default_size: f32, color: &str, theme: &Theme) -> Div {
        match &block.html {
            Some(HtmlAdapter::Adapted { nodes }) => {
                self.render_html_nodes(nodes, default_size, color, theme)
            }
            Some(HtmlAdapter::Unsupported { raw }) => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(pill(
                    "html adapter",
                    "degraded unsupported html".to_string(),
                    theme,
                ))
                .child(
                    div()
                        .p(px(14.0))
                        .bg(rgb_hex(&theme.palette.code_background))
                        .rounded(px(10.0))
                        .text_size(px(13.0))
                        .text_color(rgb_hex(&theme.palette.text_muted))
                        .child(raw.clone()),
                ),
            None => div()
                .text_size(px(default_size))
                .text_color(rgb_hex(color))
                .child(block.rendered.clone()),
        }
    }

    fn render_html_nodes(&self, nodes: &[HtmlNode], default_size: f32, default_color: &str, theme: &Theme) -> Div {
        nodes.iter().fold(
            div().flex().flex_wrap().items_start().gap(px(0.0)),
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
                        .bg(rgb_hex(&theme.palette.code_background))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(rgb_hex(&theme.palette.panel_border))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb_hex(&theme.palette.accent))
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
                                        .text_color(rgb_hex(&theme.palette.text_primary))
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
                                        .text_color(rgb_hex(&theme.palette.text_muted))
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
}

impl SolaRoot {
    fn trigger_typst_renders(&mut self, cx: &mut Context<Self>) {
        let requests = self.workspace.update(cx, |workspace, _cx| {
            workspace
                .document()
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
                .collect::<Vec<_>>()
        });

        for (index, block_source, kind, source, cache_key) in requests {
            if let Some(cached) = self.typst_cache.get(&cache_key).cloned() {
                self.workspace.update(cx, |workspace, cx| {
                    if apply_cached_typst_adapter(workspace.document_mut(), &cache_key, cached) > 0 {
                        cx.notify();
                    }
                });
                continue;
            }

            if !should_start_typst_compile(&self.typst_cache, &self.typst_in_flight, &cache_key) {
                continue;
            }

            self.typst_in_flight.insert(cache_key.clone());
            let workspace = self.workspace.clone();
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

                        workspace.update(cx, |workspace, cx| {
                            if apply_completed_typst_work(
                                workspace.document_mut(),
                                &cache_key,
                                index,
                                &block_source,
                                next_adapter.clone(),
                            ) > 0
                            {
                                cx.notify();
                            }
                        });
                    });
                }
            })
            .detach();
        }
    }

    fn handle_focused_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.cursor_visible = true;
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;
        let primary = modifiers.control || modifiers.platform;

        if primary && key.eq_ignore_ascii_case("t") {
            self.toggle_theme(cx);
            return true;
        }

        self.workspace.update(cx, |workspace, cx| {
            let theme = workspace.theme().clone();

            if primary && modifiers.shift && key.eq_ignore_ascii_case("z") {
                return workspace.update_document(cx, |doc| doc.redo());
            }

            if primary && key.eq_ignore_ascii_case("y") {
                return workspace.update_document(cx, |doc| doc.redo());
            }

            if primary && key.eq_ignore_ascii_case("z") {
                return workspace.update_document(cx, |doc| doc.undo());
            }

            if modifiers.alt && key.eq_ignore_ascii_case("up") {
                return workspace.update_document(cx, |doc| {
                    if doc.focused_has_draft() {
                        doc.apply_focused_draft();
                    }
                    doc.focus_previous()
                });
            }

            if modifiers.alt && key.eq_ignore_ascii_case("down") {
                return workspace.update_document(cx, |doc| {
                    if doc.focused_has_draft() {
                        doc.apply_focused_draft();
                    }
                    doc.focus_next()
                });
            }

            if key.eq_ignore_ascii_case("left") {
                return workspace.update_document(cx, |doc| doc.move_cursor_left(modifiers.shift));
            }

            if key.eq_ignore_ascii_case("right") {
                return workspace.update_document(cx, |doc| doc.move_cursor_right(modifiers.shift));
            }

            if key.eq_ignore_ascii_case("up") {
                return workspace.update_document(cx, |doc| {
                    if let Some(target) = self.soft_wrapped_vertical_target(-1, doc, &theme, window) {
                        return doc.set_focused_cursor(target, modifiers.shift);
                    }
                    doc.move_cursor_up(modifiers.shift)
                });
            }

            if key.eq_ignore_ascii_case("down") {
                return workspace.update_document(cx, |doc| {
                    if let Some(target) = self.soft_wrapped_vertical_target(1, doc, &theme, window) {
                        return doc.set_focused_cursor(target, modifiers.shift);
                    }
                    doc.move_cursor_down(modifiers.shift)
                });
            }

            if key.eq_ignore_ascii_case("home") {
                return workspace.update_document(cx, |doc| {
                    if let Some(target) = self.visual_line_edge_offset(doc, &theme, window, false) {
                        return doc.set_focused_cursor(target, modifiers.shift);
                    }
                    false
                });
            }

            if key.eq_ignore_ascii_case("end") {
                return workspace.update_document(cx, |doc| {
                    if let Some(target) = self.visual_line_edge_offset(doc, &theme, window, true) {
                        return doc.set_focused_cursor(target, modifiers.shift);
                    }
                    false
                });
            }

            if primary && key.eq_ignore_ascii_case("a") {
                return workspace.update_document(cx, |doc| {
                    doc.select_all();
                    true
                });
            }

            if primary && key.eq_ignore_ascii_case("n") {
                return workspace.update_document(cx, |doc| {
                    doc.insert_paragraph_after_focused(
                        "Inserted via keyboard shortcut as a structure-editing prototype.",
                    )
                });
            }

            if primary && key.eq_ignore_ascii_case("d") {
                return workspace.update_document(cx, |doc| doc.duplicate_focused_block());
            }

            if primary && key.eq_ignore_ascii_case("backspace") {
                return workspace.update_document(cx, |doc| doc.delete_focused_block());
            }

            if primary && key.eq_ignore_ascii_case("s") {
                workspace.update_document(cx, |doc| {
                    doc.apply_focused_draft();
                });
                workspace.save_current_file(cx);
                return true;
            }

            if key.eq_ignore_ascii_case("escape") {
                return workspace.update_document(cx, |doc| {
                    doc.revert_focused_draft();
                    true
                });
            }

            if key.eq_ignore_ascii_case("backspace") {
                return workspace.update_document(cx, |doc| doc.delete_at_cursor_in_focused_draft());
            }

            if key.eq_ignore_ascii_case("enter") {
                return workspace.update_document(cx, |doc| doc.push_char_to_focused_draft('\n'));
            }

            if !modifiers.control && !modifiers.alt && !modifiers.platform {
                if let Some(ch) = event.keystroke.key_char.as_deref() {
                    let mut chars = ch.chars();
                    if let Some(single) = chars.next() {
                        if chars.next().is_none() {
                            return workspace.update_document(cx, |doc| doc.push_char_to_focused_draft(single));
                        }
                    }
                }
            }

            false
        })
    }

    fn soft_wrapped_vertical_target(
        &self,
        delta: isize,
        document: &DocumentModel,
        theme: &Theme,
        window: &mut Window,
    ) -> Option<usize> {
        let text = document.focused_text()?;
        let cursor = document.focused_cursor()?;
        let style = FocusedEditorStyle::from_theme(theme);
        let wrap_width = approximate_editor_wrap_width(window.bounds().size.width);
        let lines = shape_focused_lines(
            window,
            text,
            &style,
            rgb_hex(&theme.palette.text_primary),
            wrap_width,
        )?;

        move_cursor_vertical_visual(&lines, cursor.head, delta, style.line_height)
    }

    fn visual_line_edge_offset(
        &self,
        document: &DocumentModel,
        theme: &Theme,
        window: &mut Window,
        line_end: bool,
    ) -> Option<usize> {
        let text = document.focused_text()?;
        let cursor = document.focused_cursor()?;
        let style = FocusedEditorStyle::from_theme(theme);
        let wrap_width = approximate_editor_wrap_width(window.bounds().size.width);
        let lines = shape_focused_lines(
            window,
            text,
            &style,
            rgb_hex(&theme.palette.text_primary),
            wrap_width,
        )?;

        let visual = visual_line_ranges(&lines);
        visual_line_edge_offset(&visual, cursor.head, line_end)
    }
}

impl Render for SolaRoot {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.workspace.read(cx).theme().clone();

        div()
            .size_full()
            .bg(rgb_hex(&theme.palette.app_background))
            .text_color(rgb_hex(&theme.palette.text_primary))
            .on_action(cx.listener(|this, _action: &Open, window, cx| {
                this.open_project(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &Save, _window, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.save_current_file(cx);
                });
            }))
            .on_action(cx.listener(|_this, _action: &Quit, _window, cx| {
                cx.quit();
            }))
            .on_action(cx.listener(|this, _action: &Undo, _window, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.update_document(cx, |doc| {
                        doc.undo();
                    });
                });
            }))
            .on_action(cx.listener(|this, _action: &Redo, _window, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.update_document(cx, |doc| {
                        doc.redo();
                    });
                });
            }))
            .on_action(cx.listener(|this, _action: &ToggleTheme, _window, cx| {
                this.toggle_theme(cx);
            }))
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
                            .flex_row()
                            .child(self.project_panel.clone())
                            .child(self.render_document_surface(cx)),
                    ),
            )
    }
}


pub(crate) fn rgb_hex(hex: &str) -> Hsla {
    rgb(parse_hex_color(hex).unwrap_or(0xffffff)).into()
}

fn section_title(title: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(14.0))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb_hex(&theme.palette.text_primary))
        .child(title.to_string())
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

fn plan_block_click(current_index: usize, target_index: usize, has_draft: bool) -> BlockClickPlan {
    BlockClickPlan {
        apply_draft: has_draft && current_index != target_index,
        switch_block_focus: current_index != target_index,
        refresh_window_focus: true,
    }
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
        BlockClickPlan, apply_cached_typst_adapter, apply_completed_typst_work, apply_typst_result,
        plan_block_click, should_start_typst_compile, typst_adapter_from_result, typst_cache_key,
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

    #[test]
    fn plan_block_click_keeps_focus_refresh_for_same_block() {
        assert_eq!(
            plan_block_click(2, 2, true),
            BlockClickPlan {
                apply_draft: false,
                switch_block_focus: false,
                refresh_window_focus: true,
            }
        );
        assert_eq!(
            plan_block_click(1, 3, true),
            BlockClickPlan {
                apply_draft: true,
                switch_block_focus: true,
                refresh_window_focus: true,
            }
        );
    }
}
