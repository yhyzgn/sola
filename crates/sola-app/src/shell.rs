use crate::actions::{Open, Quit, Redo, Save, ToggleTheme, Undo};
use crate::focused_editor::{
    FocusedEditorElement, FocusedEditorStyle, approximate_editor_wrap_width,
    move_cursor_vertical_visual, shape_focused_lines, spans_to_runs, visual_line_edge_offset,
    visual_line_ranges,
};
use crate::project_panel::ProjectPanel;
use crate::workspace::{Workspace, WorkspaceEvent};
use crate::worktree::Worktree;
use gpui::prelude::{FluentBuilder, StatefulInteractiveElement, Styled};
use gpui::{
    AppContext, Application, AsyncApp, Bounds, Context, Div, Entity, FocusHandle, FontWeight, Hsla,
    Image, ImageFormat, InteractiveElement, IntoElement, KeyBinding, Menu, MenuItem, MouseButton,
    ParentElement, Render, WeakEntity, Window,
    WindowBounds, WindowOptions, div, img, px, rgb, size,
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

pub struct SolaRoot {
    focus_handle: FocusHandle,
    this_handle: Option<WeakEntity<Self>>,
    workspace: Entity<Workspace>,
    project_panel: Entity<ProjectPanel>,
    highlighter: SyntaxHighlighter,
    typst_cache: HashMap<String, TypstAdapter>,
    typst_in_flight: HashSet<String>,
    cursor_visible: bool,
    cursor_blink_started: bool,
    active_menu: Option<&'static str>,
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
            this.open_template(sample_markdown().to_string(), cx);
        });

        cx.subscribe(&workspace, |this, _workspace, event, cx| match event {
            WorkspaceEvent::DocumentChanged | WorkspaceEvent::ActiveTabChanged => {
                this.trigger_typst_renders(cx);
                cx.notify();
            }
            WorkspaceEvent::ThemeChanged => {
                cx.notify();
            }
            WorkspaceEvent::WorktreeChanged => {}
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
            active_menu: None,
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

    fn render_menu_bar(&self, cx: &mut Context<Self>) -> Div {
        let theme = self.workspace.read(cx).theme();
        let active_menu = self.active_menu;

        div()
            .flex()
            .flex_row()
            .bg(rgb_hex(&theme.palette.panel_background))
            .border_b_1()
            .border_color(rgb_hex(&theme.palette.panel_border))
            .px(px(8.0))
            .child(self.render_menu_bar_item("File", active_menu == Some("File"), cx))
            .child(self.render_menu_bar_item("Edit", active_menu == Some("Edit"), cx))
            .child(self.render_menu_bar_item("View", active_menu == Some("View"), cx))
    }

    fn render_menu_bar_item(
        &self,
        label: &'static str,
        is_active: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.workspace.read(cx).theme();

        div()
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(4.0))
            .bg(if is_active {
                rgb_hex(&theme.palette.code_background)
            } else {
                gpui::hsla(0.0, 0.0, 0.0, 0.0)
            })
            .hover(|s| s.bg(rgb_hex(&theme.palette.code_background)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    if this.active_menu == Some(label) {
                        this.active_menu = None;
                    } else {
                        this.active_menu = Some(label);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.active_menu.is_some() && this.active_menu != Some(label) {
                    this.active_menu = Some(label);
                    cx.notify();
                }
            }))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb_hex(&theme.palette.text_primary))
                    .child(label),
            )
    }

    fn render_menu_overlay(&self, cx: &mut Context<Self>) -> Option<Div> {
        let active_menu = self.active_menu?;
        let theme = self.workspace.read(cx).theme().clone();

        // Define items based on active_menu
        let items = match active_menu {
            "File" => vec![
                ("Open...", "Ctrl+O", true),
                ("Save", "Ctrl+S", true),
                ("Separator", "", false),
                ("Quit", "Ctrl+Q", true),
            ],
            "Edit" => vec![
                ("Undo", "Ctrl+Z", true),
                ("Redo", "Ctrl+Y", true),
                ("Separator", "", false),
                ("Insert Paragraph", "Ctrl+N", true),
                ("Duplicate Block", "Ctrl+D", true),
                ("Delete Block", "Backspace", true),
            ],
            "View" => vec![("Toggle Theme", "Ctrl+T", true)],
            _ => vec![],
        };

        let x_pos = match active_menu {
            "File" => px(8.0),
            "Edit" => px(60.0),
            "View" => px(110.0),
            _ => px(0.0),
        };

        let border_color = rgb_hex(&theme.palette.panel_border);
        let bg_color = rgb_hex(&theme.palette.panel_background);

        Some(
            div()
                .absolute()
                .top(px(34.0))
                .left(x_pos)
                .bg(bg_color)
                .border_1()
                .border_color(border_color)
                .rounded(px(8.0))
                .p(px(4.0))
                .min_w(px(180.0))
                .flex()
                .flex_col()
                .children(items.into_iter().map(|(label, shortcut, enabled)| {
                    if label == "Separator" {
                        return div()
                            .h(px(1.0))
                            .bg(border_color)
                            .my(px(4.0))
                            .into_any_element();
                    }

                    self.render_overlay_item(label, shortcut, enabled, cx)
                        .into_any_element()
                })),
        )
    }

    fn render_overlay_item(
        &self,
        label: &'static str,
        shortcut: &'static str,
        _enabled: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.workspace.read(cx).theme();

        div()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(4.0))
            .hover(|s| s.bg(rgb_hex(&theme.palette.code_background)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.active_menu = None;
                    // Dispatch logic
                    match label {
                        "Open..." => this.open_project(window, cx),
                        "Save" => this.workspace.update(cx, |w, cx| w.save_current_file(cx)),
                        "Quit" => cx.quit(),
                        "Undo" => this.workspace.update(cx, |w, cx| {
                            w.update_active_document(cx, |d| {
                                d.undo();
                            });
                        }),
                        "Redo" => this.workspace.update(cx, |w, cx| {
                            w.update_active_document(cx, |d| {
                                d.redo();
                            });
                        }),
                        "Insert Paragraph" => this.workspace.update(cx, |w, cx| {
                            w.update_active_document(cx, |d| {
                                d.insert_paragraph_after_focused("New block");
                            });
                        }),
                        "Duplicate Block" => this.workspace.update(cx, |w, cx| {
                            w.update_active_document(cx, |d| {
                                d.duplicate_focused_block();
                            });
                        }),
                        "Delete Block" => this.workspace.update(cx, |w, cx| {
                            w.update_active_document(cx, |d| {
                                d.delete_focused_block();
                            });
                        }),
                        "Toggle Theme" => this.toggle_theme(cx),
                        _ => {}
                    }
                    cx.notify();
                }),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb_hex(&theme.palette.text_primary))
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb_hex(&theme.palette.text_muted))
                            .child(shortcut),
                    ),
            )
    }

    fn render_menu_mask(&self, cx: &mut Context<Self>) -> Option<Div> {
        self.active_menu.map(|_| {
            div()
                .absolute()
                .size_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.active_menu = None;
                        cx.notify();
                    }),
                )
        })
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
                    .child(pill("workspace", format!("{} crates", 4), theme))
                    .child(pill(
                        "focused block",
                        format!("#{}", workspace.active_document_ref().map_or(0, |d| d.focused_block() + 1)),
                        theme,
                    ))
                    .child(pill("roadmap", "phase 1 / 2 / 5".to_string(), theme)),
            )
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> Div {
        let workspace = self.workspace.read(cx);
        let theme = workspace.theme();
        let docs = workspace.documents();
        let active_idx = workspace.active_document_index();

        div()
            .flex()
            .flex_row()
            .bg(rgb_hex(&theme.palette.panel_background))
            .border_b_1()
            .border_color(rgb_hex(&theme.palette.panel_border))
            .children(docs.iter().enumerate().map(|(idx, doc)| {
                let is_active = Some(idx) == active_idx;
                let filename = doc.path.as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Untitled".to_string());

                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .py(px(8.0))
                    .bg(if is_active {
                        rgb_hex(&theme.palette.app_background)
                    } else {
                        gpui::hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .border_r_1()
                    .border_color(rgb_hex(&theme.palette.panel_border))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.workspace.update(cx, |w, cx| w.switch_tab(idx, cx));
                        }),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb_hex(if is_active {
                                &theme.palette.text_primary
                            } else {
                                &theme.palette.text_muted
                            }))
                            .child(filename),
                    )
                    .child(
                        div()
                            .p(px(2.0))
                            .text_size(px(10.0))
                            .hover(|s| s.text_color(rgb_hex(&theme.palette.accent)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.workspace.update(cx, |w, cx| w.close_tab(idx, cx));
                                }),
                            )
                            .child("✕"),
                    )
            }))
    }

    fn render_document_surface(&mut self, cx: &mut Context<Self>) -> Div {
        let (theme, active_doc_opt) = {
            let workspace = self.workspace.read(cx);
            (
                workspace.theme().clone(),
                workspace.active_document_ref().cloned(),
            )
        };

        if active_doc_opt.is_none() {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb_hex(&theme.palette.app_background))
                .child(
                    div()
                        .text_color(rgb_hex(&theme.palette.text_muted))
                        .child("No files open"),
                );
        }

        let document = active_doc_opt.unwrap();
        self.ensure_cursor_blink_loop(cx);

        let blocks = document.blocks().iter().enumerate().fold(
            div().flex().flex_col().gap(px(14.0)).p(px(24.0)),
            |surface, (index, block)| surface.child(self.render_block(index, block, cx)),
        );

        let previous_button = {
            let workspace = self.workspace.clone();
            let can_prev = document.focused_block() > 0;
            action_button("← previous block".to_string(), &theme, can_prev)
                .id("previous-block")
                .on_click(cx.listener(move |_this, _event, _window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.update_active_document(cx, |doc| {
                            doc.focus_previous();
                        });
                    });
                }))
        };

        let next_button = {
            let workspace = self.workspace.clone();
            let can_next = document.focused_block() + 1 < document.block_count();
            action_button("next block →".to_string(), &theme, can_next)
                .id("next-block")
                .on_click(cx.listener(move |_this, _event, _window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.update_active_document(cx, |doc| {
                            doc.focus_next();
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
                        workspace.update_active_document(cx, |doc| {
                            doc.insert_paragraph_after_focused(
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
                        workspace.update_active_document(cx, |doc| {
                            doc.duplicate_focused_block();
                        });
                    });
                }))
        };

        let delete_button = {
            let workspace = self.workspace.clone();
            let can_delete = document.block_count() > 1;
            action_button("delete block".to_string(), &theme, can_delete)
                .id("delete-block")
                .on_click(cx.listener(move |_this, _event, _window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.update_active_document(cx, |doc| {
                            doc.delete_focused_block();
                        });
                    });
                }))
        };

        let undo_button = {
            let workspace = self.workspace.clone();
            let can_undo = document.can_undo();
            action_button("undo".to_string(), &theme, can_undo)
                .id("undo")
                .on_click(cx.listener(move |_this, _event, _window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.update_active_document(cx, |doc| {
                            doc.undo();
                        });
                    });
                }))
        };

        let redo_button = {
            let workspace = self.workspace.clone();
            let can_redo = document.can_redo();
            action_button("redo".to_string(), &theme, can_redo)
                .id("redo")
                .on_click(cx.listener(move |_this, _event, _window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.update_active_document(cx, |doc| {
                            doc.redo();
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
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(section_title("STRUCTURE EDITOR", &theme))
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(8.0))
                                            .items_center()
                                            .child(pill("focused", truncate_for_pill(&focused_summary, 32), &theme))
                                            .child(pill("state", draft_label.to_string(), &theme)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(12.0))
                                    .child(previous_button)
                                    .child(next_button)
                                    .child(div().w(px(1.0)).h(px(24.0)).bg(rgb_hex(&theme.palette.panel_border)))
                                    .child(insert_button)
                                    .child(duplicate_button)
                                    .child(delete_button),
                            ),
                    ),
            )
            .child(
                div()
                    .id("main-scroll-container")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scroll()
                    .bg(rgb_hex(&theme.palette.app_background))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(32.0))
                            .p(px(32.0))
                            .max_w(px(900.0))
                            .mx_auto()
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(section_title("DOCUMENT SURFACE", &theme))
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(12.0))
                                            .child(undo_button)
                                            .child(redo_button),
                                    ),
                            )
                            .child(blocks)
                            .child(
                                div()
                                    .mt(px(24.0))
                                    .pt(px(24.0))
                                    .border_t_1()
                                    .border_color(rgb_hex(&theme.palette.panel_border))
                                    .child(section_title("KEYBOARD SHORTCUTS", &theme))
                                    .child(div().mt(px(16.0)).child(shortcut_legend(&theme))),
                            ),
                    ),
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
        let Some(document) = workspace.active_document_ref() else {
            return div().into_any_element();
        };
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
                                        workspace.update_active_document(cx, |doc| {
                                            doc.set_focused_cursor(offset, shift);
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
                        workspace.update_active_document(cx, |doc| {
                            let plan = plan_block_click(
                                doc.focused_block(),
                                index,
                                doc.focused_has_draft(),
                            );

                            if plan.apply_draft {
                                doc.apply_focused_draft();
                            }

                            if plan.switch_block_focus {
                                doc.focus_block(index);
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
            .into_any_element()
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
                .flex()
                .gap(px(12.0))
                .child(div().w(px(4.0)).bg(rgb_hex(&theme.palette.accent)).rounded_full())
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
            BlockKind::CodeFence { language } => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb_hex(&theme.palette.text_muted))
                        .child(format!(
                            "Code block · {}",
                            language.as_ref().unwrap_or(&"plain text".to_string())
                        )),
                )
                .child(
                    div()
                        .id(("code-preview", block.id))
                        .p(px(14.0))
                        .bg(rgb_hex(&theme.palette.code_background))
                        .rounded(px(10.0))
                        .overflow_x_scroll()
                        .child(
                            div()
                                .text_size(px(theme.typography.code_size as f32))
                                .text_color(rgb_hex(&theme.palette.text_primary))
                                .child(block.rendered.clone()),
                        ),
                ),
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

    fn render_html_nodes(
        &self,
        nodes: &[HtmlNode],
        default_size: f32,
        default_color: &str,
        theme: &Theme,
    ) -> Div {
        nodes.iter().fold(
            div().flex().flex_wrap().items_center().gap(px(0.0)),
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
                HtmlNode::InlineMath(math) => {
                    let cache_key = typst_cache_key(&RenderKind::Math, math);
                    if let Some(TypstAdapter::Rendered { svg }) = self.typst_cache.get(&cache_key) {
                        content.child(
                            div().mx(px(4.0)).child(
                                img(Arc::new(Image::from_bytes(
                                    ImageFormat::Svg,
                                    svg.as_bytes().to_vec(),
                                )))
                                .h(px(default_size * 1.3)),
                            ),
                        )
                    } else {
                        content.child(
                            div()
                                .text_size(px(default_size))
                                .text_color(rgb_hex(&theme.palette.accent))
                                .child(format!("${}$", math)),
                        )
                    }
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
                                                .unwrap_or_else(|| "no alt text".to_string()),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
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

    fn trigger_typst_renders(&mut self, cx: &mut Context<Self>) {
        let requests = {
            let workspace = self.workspace.read(cx);
            let Some(document) = workspace.active_document_ref() else {
                return;
            };

            let mut reqs = Vec::new();
            for (index, block) in document.blocks().iter().enumerate() {
                // 1. Check for block-level math/typst
                if matches!(block.typst, Some(TypstAdapter::Pending)) {
                    if let Some((kind, source)) = typst_render_request(block) {
                        let cache_key = typst_cache_key(&kind, &source);
                        reqs.push((index, block.source.clone(), kind, source, cache_key));
                    }
                }

                // 2. Scan for inline math in html adapted nodes
                if let Some(HtmlAdapter::Adapted { nodes }) = &block.html {
                    for node in nodes {
                        if let HtmlNode::InlineMath(source) = node {
                            let cache_key = typst_cache_key(&RenderKind::Math, source);
                            if !self.typst_cache.contains_key(&cache_key) {
                                reqs.push((
                                    index,
                                    block.source.clone(),
                                    RenderKind::Math,
                                    source.clone(),
                                    cache_key,
                                ));
                            }
                        }
                    }
                }
            }
            reqs
        };

        for (index, block_source, kind, source, cache_key) in requests {
            if let Some(cached) = self.typst_cache.get(&cache_key).cloned() {
                self.workspace.update(cx, |workspace, cx| {
                    workspace.update_active_document(cx, |document| {
                        apply_cached_typst_adapter(document, &cache_key, cached);
                    });
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
                            workspace.update_active_document(cx, |document| {
                                apply_completed_typst_work(
                                    document,
                                    &cache_key,
                                    index,
                                    &block_source,
                                    next_adapter.clone(),
                                );
                            });
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
            self.workspace.update(cx, |w, cx| w.toggle_theme(cx));
            return true;
        }

        self.workspace.update(cx, |workspace, cx| {
            let theme = workspace.theme().clone();

            if primary && modifiers.shift && key.eq_ignore_ascii_case("z") {
                return workspace.update_active_document(cx, |doc| doc.redo()).unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("y") {
                return workspace.update_active_document(cx, |doc| doc.redo()).unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("z") {
                return workspace.update_active_document(cx, |doc| doc.undo()).unwrap_or(false);
            }

            if modifiers.alt && key.eq_ignore_ascii_case("up") {
                return workspace.update_active_document(cx, |doc| {
                    if doc.focused_has_draft() {
                        doc.apply_focused_draft();
                    }
                    doc.focus_previous()
                }).unwrap_or(false);
            }

            if modifiers.alt && key.eq_ignore_ascii_case("down") {
                return workspace.update_active_document(cx, |doc| {
                    if doc.focused_has_draft() {
                        doc.apply_focused_draft();
                    }
                    doc.focus_next()
                }).unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("left") {
                return workspace.update_active_document(cx, |doc| doc.move_cursor_left(modifiers.shift)).unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("right") {
                return workspace.update_active_document(cx, |doc| doc.move_cursor_right(modifiers.shift)).unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("up") {
                return workspace.update_active_document(cx, |doc| {
                    if let Some(target) = self.soft_wrapped_vertical_target(-1, doc, &theme, window) {
                        return doc.set_focused_cursor(target, modifiers.shift);
                    }
                    doc.move_cursor_up(modifiers.shift)
                }).unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("down") {
                return workspace.update_active_document(cx, |doc| {
                    if let Some(target) = self.soft_wrapped_vertical_target(1, doc, &theme, window) {
                        return doc.set_focused_cursor(target, modifiers.shift);
                    }
                    doc.move_cursor_down(modifiers.shift)
                }).unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("home") {
                return workspace.update_active_document(cx, |doc| {
                    if let Some(target) = self.visual_line_edge_offset(doc, &theme, window, false) {
                        return doc.set_focused_cursor(target, modifiers.shift);
                    }
                    false
                }).unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("end") {
                return workspace.update_active_document(cx, |doc| {
                    if let Some(target) = self.visual_line_edge_offset(doc, &theme, window, true) {
                        return doc.set_focused_cursor(target, modifiers.shift);
                    }
                    false
                }).unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("a") {
                return workspace.update_active_document(cx, |doc| {
                    doc.select_all();
                    true
                }).unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("n") {
                return workspace.update_active_document(cx, |doc| {
                    doc.insert_paragraph_after_focused(
                        "Inserted via keyboard shortcut as a structure-editing prototype.",
                    )
                }).unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("d") {
                return workspace.update_active_document(cx, |doc| doc.duplicate_focused_block()).unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("backspace") {
                return workspace.update_active_document(cx, |doc| doc.delete_focused_block()).unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("s") {
                workspace.update_active_document(cx, |doc| {
                    doc.apply_focused_draft();
                });
                workspace.save_current_file(cx);
                return true;
            }

            if key.eq_ignore_ascii_case("escape") {
                return workspace.update_active_document(cx, |doc| {
                    doc.revert_focused_draft();
                    true
                }).unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("backspace") {
                return workspace.update_active_document(cx, |doc| doc.delete_at_cursor_in_focused_draft()).unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("enter") {
                return workspace.update_active_document(cx, |doc| doc.push_char_to_focused_draft('\n')).unwrap_or(false);
            }

            if !modifiers.control && !modifiers.alt && !modifiers.platform {
                if let Some(ch) = event.keystroke.key_char.as_deref() {
                    let mut chars = ch.chars();
                    if let Some(single) = chars.next() {
                        if chars.next().is_none() {
                            return workspace.update_active_document(cx, |doc| doc.push_char_to_focused_draft(single)).unwrap_or(false);
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
            .relative()
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
                    workspace.update_active_document(cx, |doc| {
                        doc.undo();
                    });
                });
            }))
            .on_action(cx.listener(|this, _action: &Redo, _window, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.update_active_document(cx, |doc| {
                        doc.redo();
                    });
                });
            }))
            .on_action(cx.listener(|this, _action: &ToggleTheme, _window, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.toggle_theme(cx);
                });
            }))
            .child(self.render_menu_bar(cx))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .child(self.render_header(cx))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_row()
                            .min_h_0()
                            .child(self.project_panel.clone())
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .min_w_0()
                                    .child(self.render_tab_bar(cx))
                                    .child(self.render_document_surface(cx)),
                            ),
                    ),
            )
            .when_some(self.render_menu_mask(cx), |this, mask| this.child(mask))
            .when_some(self.render_menu_overlay(cx), |this, overlay| {
                this.child(overlay)
            })
    }
}

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
                    size(px(1280.0), px(800.0)),
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
