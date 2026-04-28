use crate::actions::{
    CloseTab, NewFile, Open, OpenFile, OpenFolder, Preferences, Quit, Redo, Save, SaveAs,
    ToggleTheme, Undo,
};
use crate::focused_editor::{
    FocusedEditorElement, FocusedEditorStyle, generate_editor_blocks,
};
use crate::project_panel::ProjectPanel;
use crate::workspace::{Workspace, WorkspaceEvent};
use crate::worktree::Worktree;
use gpui::prelude::{FluentBuilder, StatefulInteractiveElement, Styled};
use gpui::{
    AppContext, Application, AsyncApp, Bounds, Context, Div, Entity, FocusHandle, FontWeight, Hsla,
    InteractiveElement, IntoElement, KeyBinding, Menu, MenuItem, MouseButton,
    ParentElement, Render, WeakEntity, Window, WindowBounds, WindowOptions, div, px, rgb,
    size,
};

use sola_core::sample_markdown;
use sola_document::{
    BlockKind, CursorState, DocumentBlock, DocumentModel, HtmlAdapter, HtmlNode, TypstAdapter,
};
use sola_theme::{Theme, parse_hex_color};
use sola_typst::{RenderKind, TypstError, compile_to_svg};
use std::{
    collections::{HashMap, HashSet},
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
    typst_cache: HashMap<String, TypstAdapter>,
    typst_in_flight: HashSet<String>,
    cursor_visible: bool,
    cursor_blink_started: bool,
    active_menu: Option<&'static str>,
    active_submenu: Option<&'static str>,
    show_preferences: bool,
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
            this.open_template(DocumentModel::from_markdown(sample_markdown()), cx);
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
            typst_cache: HashMap::new(),
            typst_in_flight: HashSet::new(),
            cursor_visible: true,
            cursor_blink_started: false,
            active_menu: None,
            active_submenu: None,
            show_preferences: false,
        };

        this.trigger_typst_renders(cx);
        this
    }

    fn open_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        use gpui::PathPromptOptions;
        self.open_project_with_options(
            PathPromptOptions {
                files: true,
                directories: true,
                multiple: false,
                prompt: Some("Open File or Folder".into()),
            },
            cx,
        );
    }

    fn open_file_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        use gpui::PathPromptOptions;
        self.open_project_with_options(
            PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some("Open File".into()),
            },
            cx,
        );
    }

    fn open_folder_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        use gpui::PathPromptOptions;
        self.open_project_with_options(
            PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: Some("Open Folder".into()),
            },
            cx,
        );
    }

    fn open_project_with_options(
        &mut self,
        options: gpui::PathPromptOptions,
        cx: &mut Context<Self>,
    ) {
        let this_handle = self.this_handle.clone();
        let paths_rx = cx.prompt_for_paths(options);

        cx.spawn(|_this, cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                if let Ok(Ok(Some(paths))) = paths_rx.await {
                    if let Some(path) = paths.first() {
                        if let Some(this_handle) = this_handle {
                            let path = path.clone();
                            let _ = this_handle.update(&mut cx, |this, cx| {
                                this.open_path(path, cx);
                            });
                        }
                    }
                }
            }
        })
        .detach();
    }

    fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let is_dir = path.is_dir();
        let target_dir = if is_dir {
            path.clone()
        } else {
            path.parent().unwrap_or(&path).to_path_buf()
        };

        // 1. Smart Worktree Update
        let current_worktree_path = self
            .workspace
            .read(cx)
            .worktree()
            .read(cx)
            .abs_path()
            .to_path_buf();
        if target_dir != current_worktree_path {
            let worktree = Worktree::local(target_dir, cx);
            self.workspace.update(cx, |workspace, cx| {
                workspace.update_worktree(worktree, cx);
            });
        }

        // 2. Async Reading and Parsing (Thread-safe)
        if !is_dir {
            let workspace = self.workspace.clone();
            let path = path.clone();

            cx.spawn(|_this, cx: &mut gpui::AsyncApp| {
                let cx = cx.clone();
                let background = cx.background_executor().clone();
                async move {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let document = background
                            .spawn(async move { DocumentModel::from_markdown(content) })
                            .await;

                        let _ = cx.update(|cx| {
                            workspace.update(cx, |workspace, cx| {
                                workspace.open_file(path, document, cx);
                            });
                        });
                    }
                }
            })
            .detach();
        }

        cx.notify();
    }

    fn save_as_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let this_handle = self.this_handle.clone();

        let base_path = self
            .workspace
            .read(cx)
            .current_path()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));
        let path_rx = cx.prompt_for_new_path(&base_path, Some("untitled.md"));

        cx.spawn(|_this, cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                if let Ok(Ok(Some(path))) = path_rx.await {
                    if let Some(this_handle) = this_handle {
                        let _ = this_handle.update(&mut cx, |this, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.save_as(path, cx);
                            });
                        });
                    }
                }
            }
        })
        .detach();
    }

    fn export_document_as(
        &mut self,
        format: sola_export::ExportFormat,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.read(cx);
        let Some(document) = workspace.active_document_ref() else {
            return; // No document to export
        };
        let theme = workspace.theme().clone();

        let base_path = workspace
            .current_path()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));

        let default_name = match format {
            sola_export::ExportFormat::Markdown => "untitled.md",
            sola_export::ExportFormat::Html => "untitled.html",
        };

        let path_rx = cx.prompt_for_new_path(&base_path, Some(default_name));

        // Need to clone document data before spawning because we can't move DocumentModel reference easily across threads safely without locking.
        // But DocumentModel is Clone.
        let document = document.clone();

        cx.spawn(move |_this, cx: &mut gpui::AsyncApp| {
            let background = cx.background_executor().clone();
            async move {
                if let Ok(Ok(Some(path))) = path_rx.await {
                    let artifact = background
                        .spawn(
                            async move { sola_export::export_document(&document, &theme, format) },
                        )
                        .await;

                    let _ = std::fs::write(&path, artifact.bytes);
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
            .child(self.render_menu_bar_item("Themes", active_menu == Some("Themes"), cx))
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
                        this.active_submenu = None;
                    } else {
                        this.active_menu = Some(label);
                        this.active_submenu = None;
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.active_menu.is_some() && this.active_menu != Some(label) {
                    this.active_menu = Some(label);
                    this.active_submenu = None;
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
                ("New", "Ctrl+N", true),
                ("Separator", "", false),
                ("Open File...", "Ctrl+O", true),
                ("Open Folder...", "Ctrl+Shift+O", true),
                ("Open Recent", ">", true),
                ("Separator", "", false),
                ("Save", "Ctrl+S", true),
                ("Save As...", "Ctrl+Shift+S", true),
                ("Separator", "", false),
                ("Import", ">", true),
                ("Export", ">", true),
                ("Separator", "", false),
                ("Preferences", "Ctrl+,", true),
                ("Separator", "", false),
                ("Close Tab", "Ctrl+W", true),
                ("Quit", "Ctrl+Q", true),
            ],
            "Edit" => vec![
                ("Undo", "Ctrl+Z", true),
                ("Redo", "Ctrl+Y", true),
                ("Separator", "", false),
                ("Cut", "Ctrl+X", true),
                ("Copy", "Ctrl+C", true),
                ("Paste", "Ctrl+V", true),
                ("Separator", "", false),
                ("Select All", "Ctrl+A", true),
                ("Separator", "", false),
                ("Insert Paragraph", "Ctrl+N", true),
                ("Duplicate Block", "Ctrl+D", true),
                ("Delete Block", "Backspace", true),
            ],
            "View" => vec![
                ("Toggle Sidebar", "Ctrl+\\", true),
                ("Toggle Outline", "", true),
                ("Separator", "", false),
                ("Source Code Mode", "Ctrl+/", true),
                ("Focus Mode", "F8", true),
                ("Typewriter Mode", "F9", true),
            ],
            "Themes" => vec![("Sola Dark", "", true), ("Sola Light", "", true)],
            _ => vec![],
        };

        let x_pos = match active_menu {
            "File" => px(8.0),
            "Edit" => px(60.0),
            "View" => px(110.0),
            "Themes" => px(170.0),
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
                .min_w(px(220.0))
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

                    if label == "Open Recent" || label == "Import" || label == "Export" {
                        return self
                            .render_cascading_menu_item(label, cx)
                            .into_any_element();
                    }

                    self.render_overlay_item(label, shortcut, enabled, cx)
                        .into_any_element()
                })),
        )
    }

    fn render_cascading_menu_item(&self, label: &'static str, cx: &mut Context<Self>) -> Div {
        let theme = self.workspace.read(cx).theme().clone();
        let is_active = self.active_submenu == Some(label);

        div()
            .relative()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(4.0))
            .hover(|s| s.bg(rgb_hex("#3a3a3a")))
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                this.active_submenu = Some(label);
                cx.notify();
            }))
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
                            .child(">"),
                    ),
            )
            .when(is_active, |this| {
                this.child(self.render_cascading_submenu(label, cx))
            })
    }

    fn render_cascading_submenu(&self, label: &'static str, cx: &mut Context<Self>) -> Div {
        let workspace = self.workspace.read(cx);
        let theme = workspace.theme();

        type MenuAction =
            Box<dyn Fn(&mut SolaRoot, &mut Window, &mut Context<SolaRoot>) + Send + Sync>;

        let items: Vec<(String, MenuAction)> = match label {
            "Open Recent" => {
                let mut results: Vec<(String, MenuAction)> = Vec::new();
                for path in workspace.recent_paths() {
                    let path = path.clone();
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.to_string_lossy().to_string());
                    results.push((
                        name,
                        Box::new(
                            move |this: &mut SolaRoot,
                                  _window: &mut Window,
                                  cx: &mut Context<SolaRoot>| {
                                this.open_path(path.clone(), cx);
                            },
                        ) as MenuAction,
                    ));
                }

                results.push((
                    "Separator".to_string(),
                    Box::new(|_: &mut SolaRoot, _: &mut Window, _: &mut Context<SolaRoot>| {})
                        as MenuAction,
                ));
                results.push((
                    "Clear Items".to_string(),
                    Box::new(
                        |this: &mut SolaRoot, _window: &mut Window, cx: &mut Context<SolaRoot>| {
                            this.workspace.update(cx, |w, cx| w.clear_recent_paths(cx));
                        },
                    ) as MenuAction,
                ));
                results
            }
            "Import" => vec![
                (
                    "Markdown...".to_string(),
                    Box::new(|_: &mut SolaRoot, _: &mut Window, _: &mut Context<SolaRoot>| {})
                        as MenuAction,
                ),
                (
                    "HTML...".to_string(),
                    Box::new(|_: &mut SolaRoot, _: &mut Window, _: &mut Context<SolaRoot>| {})
                        as MenuAction,
                ),
            ],
            "Export" => vec![
                (
                    "Markdown...".to_string(),
                    Box::new(
                        |this: &mut SolaRoot, window: &mut Window, cx: &mut Context<SolaRoot>| {
                            this.export_document_as(
                                sola_export::ExportFormat::Markdown,
                                window,
                                cx,
                            );
                        },
                    ) as MenuAction,
                ),
                (
                    "HTML...".to_string(),
                    Box::new(
                        |this: &mut SolaRoot, window: &mut Window, cx: &mut Context<SolaRoot>| {
                            this.export_document_as(sola_export::ExportFormat::Html, window, cx);
                        },
                    ) as MenuAction,
                ),
            ],
            _ => vec![],
        };

        div()
            .absolute()
            .top(px(-4.0))
            .left(px(216.0))
            .bg(rgb_hex(&theme.palette.panel_background))
            .border_1()
            .border_color(rgb_hex(&theme.palette.panel_border))
            .rounded(px(8.0))
            .p(px(4.0))
            .min_w(px(240.0))
            .flex()
            .flex_col()
            .children(items.into_iter().map(|(label, action)| {
                if label == "Separator" {
                    return div()
                        .h(px(1.0))
                        .bg(rgb_hex(&theme.palette.panel_border))
                        .my(px(4.0))
                        .into_any_element();
                }

                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(4.0))
                    .hover(|s| s.bg(rgb_hex("#3a3a3a")))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            action(this, window, cx);
                            this.active_menu = None;
                            this.active_submenu = None;
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb_hex(&theme.palette.text_primary))
                            .child(label),
                    )
                    .into_any_element()
            }))
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
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.active_submenu.is_some() {
                    this.active_submenu = None;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.active_menu = None;
                    this.active_submenu = None;
                    // Dispatch logic
                    match label {
                        "New" => this.workspace.update(cx, |w, cx| {
                            w.open_template(DocumentModel::from_markdown(""), cx)
                        }),
                        "Open File..." => this.open_file_dialog(window, cx),
                        "Open Folder..." => this.open_folder_dialog(window, cx),
                        "Save" => this.workspace.update(cx, |w, cx| w.save_current_file(cx)),
                        "Save As..." => this.save_as_project(window, cx),
                        "Close Tab" => this.workspace.update(cx, |w, cx| {
                            if let Some(idx) = w.active_document_index() {
                                w.close_tab(idx, cx);
                            }
                        }),
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
                        "Sola Dark" => {
                            this.workspace.update(cx, |w, cx| {
                                if w.theme_mode() != crate::workspace::ThemeMode::Dark {
                                    w.toggle_theme(cx);
                                }
                            });
                        }
                        "Sola Light" => {
                            this.workspace.update(cx, |w, cx| {
                                if w.theme_mode() != crate::workspace::ThemeMode::Light {
                                    w.toggle_theme(cx);
                                }
                            });
                        }
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

    fn render_menu_mask(&self, cx: &mut Context<Self>) -> Option<gpui::Stateful<Div>> {
        self.active_menu.map(|_| {
            div()
                .id("menu-mask")
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .size_full()
                .bg(gpui::hsla(0.0, 0.0, 0.0, 0.001))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.active_menu = None;
                        this.active_submenu = None;
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, _, cx| {
                        this.active_menu = None;
                        this.active_submenu = None;
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

    fn render_preferences_modal(&self, cx: &mut Context<Self>) -> Option<gpui::Stateful<Div>> {
        if !self.show_preferences {
            return None;
        }

        let theme = self.workspace.read(cx).theme().clone();

        Some(
            div()
                .id("preferences-modal-mask")
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .size_full()
                .bg(gpui::hsla(0.0, 0.0, 0.0, 0.4))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.show_preferences = false;
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .w(px(600.0))
                        .bg(rgb_hex(&theme.palette.panel_background))
                        .border_1()
                        .border_color(rgb_hex(&theme.palette.panel_border))
                        .rounded(px(12.0))
                        .p(px(24.0))
                        .flex()
                        .flex_col()
                        .gap(px(24.0))
                        .on_mouse_down(MouseButton::Left, |_, _, _| {}) // Stop propagation
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_size(px(18.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb_hex(&theme.palette.text_primary))
                                        .child("Preferences"),
                                )
                                .child(
                                    div()
                                        .p(px(4.0))
                                        .text_size(px(14.0))
                                        .text_color(rgb_hex(&theme.palette.text_muted))
                                        .cursor_pointer()
                                        .hover(|s| {
                                            s.text_color(rgb_hex(&theme.palette.text_primary))
                                        })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _, cx| {
                                                this.show_preferences = false;
                                                cx.notify();
                                            }),
                                        )
                                        .child("✕"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .child(section_title("GENERAL", &theme))
                                .child(
                                    div()
                                        .flex()
                                        .justify_between()
                                        .items_center()
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .text_color(rgb_hex(&theme.palette.text_primary))
                                                .child("Color Theme"),
                                        )
                                        .child(
                                            action_button(
                                                format!(
                                                    "Toggle (Currently {})",
                                                    self.workspace.read(cx).theme_mode().label()
                                                ),
                                                &theme,
                                                true,
                                            )
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _, _, cx| {
                                                    this.toggle_theme(cx);
                                                }),
                                            ),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .child(section_title("KEYBOARD SHORTCUTS", &theme))
                                .child(shortcut_legend(&theme)),
                        ),
                ),
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
                let filename = doc
                    .path
                    .as_ref()
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

        let editor_style = FocusedEditorStyle::from_theme(&theme);

        let focused_block_idx = document.focused_block();
        let local_cursor = document.focused_cursor().cloned().unwrap_or_default();
        let global_cursor_head = document
            .block_local_to_global_offset(focused_block_idx, local_cursor.head)
            .unwrap_or(0);
        let global_cursor_anchor = local_cursor
            .anchor
            .and_then(|a| document.block_local_to_global_offset(focused_block_idx, a));

        let global_cursor = Some(global_cursor_head);
        let blocks = generate_editor_blocks(&document, global_cursor, &editor_style, &theme);

        let selection_color = rgb_hex(&theme.palette.selection);
        let cursor_color = rgb_hex(&theme.palette.cursor);
        let on_cursor_handle = self.this_handle.clone();
        let focus_handle = self.focus_handle.clone();

        let editor_element = FocusedEditorElement::new(
            blocks,
            self.typst_cache.clone(),
            editor_style,
            Some(CursorState {
                head: global_cursor_head,
                anchor: global_cursor_anchor,
            }),
            self.cursor_visible,
            selection_color,
            cursor_color,
        )
        .on_cursor_move(move |global_offset, shift, window, cx| {
            if let Some(this_handle) = &on_cursor_handle {
                let _ = this_handle.update(cx, |this, cx| {
                    this.workspace.update(cx, |workspace, cx| {
                        window.focus(&focus_handle);
                        this.cursor_visible = true;
                        workspace.update_active_document(cx, |doc| {
                            if let Some((block_idx, local_offset)) =
                                doc.global_offset_to_block_local(global_offset)
                            {
                                if doc.focused_block() != block_idx {
                                    if doc.focused_has_draft() {
                                        doc.apply_focused_draft();
                                    }
                                    doc.focus_block(block_idx);
                                }
                                doc.set_focused_cursor(local_offset, shift);
                            }
                        });
                    });
                });
            }
        });

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
                            .flex_1()
                            .py(px(32.0))
                            .max_w(px(900.0))
                            .mx_auto()
                            .child(editor_element),
                    ),
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

        let mut processed_cache_keys = HashSet::new();
        for (index, block_source, kind, source, cache_key) in requests {
            if self.typst_cache.contains_key(&cache_key)
                && !processed_cache_keys.contains(&cache_key)
            {
                if let Some(cached) = self.typst_cache.get(&cache_key).cloned() {
                    self.workspace.update(cx, |workspace, cx| {
                        workspace.update_active_document(cx, |document| {
                            apply_cached_typst_adapter(document, &cache_key, cached);
                        });
                    });
                    processed_cache_keys.insert(cache_key.clone());
                }
                continue;
            }

            if self.typst_cache.contains_key(&cache_key) {
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
        _window: &mut Window,
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
            if primary && modifiers.shift && key.eq_ignore_ascii_case("z") {
                return workspace
                    .update_active_document(cx, |doc| doc.redo())
                    .unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("y") {
                return workspace
                    .update_active_document(cx, |doc| doc.redo())
                    .unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("z") {
                return workspace
                    .update_active_document(cx, |doc| doc.undo())
                    .unwrap_or(false);
            }

            if modifiers.alt && key.eq_ignore_ascii_case("up") {
                return workspace
                    .update_active_document(cx, |doc| {
                        if doc.focused_has_draft() {
                            doc.apply_focused_draft();
                        }
                        doc.focus_previous()
                    })
                    .unwrap_or(false);
            }

            if modifiers.alt && key.eq_ignore_ascii_case("down") {
                return workspace
                    .update_active_document(cx, |doc| {
                        if doc.focused_has_draft() {
                            doc.apply_focused_draft();
                        }
                        doc.focus_next()
                    })
                    .unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("left") {
                return workspace
                    .update_active_document(cx, |doc| doc.move_cursor_left(modifiers.shift))
                    .unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("right") {
                return workspace
                    .update_active_document(cx, |doc| doc.move_cursor_right(modifiers.shift))
                    .unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("up") {
                return workspace
                    .update_active_document(cx, |doc| doc.move_cursor_up(modifiers.shift))
                    .unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("down") {
                return workspace
                    .update_active_document(cx, |doc| doc.move_cursor_down(modifiers.shift))
                    .unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("home") {
                return workspace
                    .update_active_document(cx, |doc| {
                        doc.set_focused_cursor(0, modifiers.shift)
                    })
                    .unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("end") {
                return workspace
                    .update_active_document(cx, |doc| {
                        let len = doc.focused_text().map(|t| t.len()).unwrap_or(0);
                        doc.set_focused_cursor(len, modifiers.shift)
                    })
                    .unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("a") {
                return workspace
                    .update_active_document(cx, |doc| {
                        doc.select_all();
                        true
                    })
                    .unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("n") {
                return workspace
                    .update_active_document(cx, |doc| {
                        doc.insert_paragraph_after_focused(
                            "Inserted via keyboard shortcut as a structure-editing prototype.",
                        )
                    })
                    .unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("d") {
                return workspace
                    .update_active_document(cx, |doc| doc.duplicate_focused_block())
                    .unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("backspace") {
                return workspace
                    .update_active_document(cx, |doc| doc.delete_focused_block())
                    .unwrap_or(false);
            }

            if primary && key.eq_ignore_ascii_case("s") {
                workspace.update_active_document(cx, |doc| {
                    doc.apply_focused_draft();
                });
                workspace.save_current_file(cx);
                return true;
            }

            if key.eq_ignore_ascii_case("escape") {
                return workspace
                    .update_active_document(cx, |doc| {
                        doc.revert_focused_draft();
                        true
                    })
                    .unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("backspace") {
                return workspace
                    .update_active_document(cx, |doc| doc.delete_at_cursor_in_focused_draft())
                    .unwrap_or(false);
            }

            if key.eq_ignore_ascii_case("enter") {
                return workspace
                    .update_active_document(cx, |doc| doc.push_char_to_focused_draft('\n'))
                    .unwrap_or(false);
            }

            if !modifiers.control && !modifiers.alt && !modifiers.platform {
                if let Some(ch) = event.keystroke.key_char.as_deref() {
                    let mut chars = ch.chars();
                    if let Some(single) = chars.next() {
                        if chars.next().is_none() {
                            return workspace
                                .update_active_document(cx, |doc| {
                                    doc.push_char_to_focused_draft(single)
                                })
                                .unwrap_or(false);
                        }
                    }
                }
            }

            false
        })
    }
}

impl Render for SolaRoot {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.workspace.read(cx).theme().clone();

        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb_hex(&theme.palette.app_background))
            .text_color(rgb_hex(&theme.palette.text_primary))
            .on_action(cx.listener(|this, _action: &Open, window, cx| {
                this.open_project(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &OpenFile, window, cx| {
                this.open_file_dialog(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &OpenFolder, window, cx| {
                this.open_folder_dialog(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &Save, _window, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.save_current_file(cx);
                });
            }))
            .on_action(cx.listener(|this, _action: &SaveAs, window, cx| {
                this.save_as_project(window, cx);
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
                this.toggle_theme(cx);
            }))
            .on_action(cx.listener(|this, _action: &CloseTab, _window, cx| {
                this.workspace.update(cx, |w, cx| {
                    if let Some(idx) = w.active_document_index() {
                        w.close_tab(idx, cx);
                    }
                });
            }))
            .on_action(cx.listener(|this, _action: &Preferences, _window, cx| {
                this.show_preferences = !this.show_preferences;
                cx.notify();
            }))
            .child(self.render_menu_bar(cx))
            .child(
                div().flex_1().flex().flex_col().min_h_0().child(
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
            .when_some(self.render_preferences_modal(cx), |this, modal| {
                this.child(modal)
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
                    MenuItem::action("New", NewFile),
                    MenuItem::separator(),
                    MenuItem::action("Open...", OpenFile),
                    MenuItem::action("Open Folder...", OpenFolder),
                    MenuItem::separator(),
                    MenuItem::action("Save", Save),
                    MenuItem::action("Save As...", SaveAs),
                    MenuItem::separator(),
                    MenuItem::action("Close", CloseTab),
                    MenuItem::action("Quit", Quit),
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
            KeyBinding::new("cmd-n", NewFile, None),
            KeyBinding::new("ctrl-n", NewFile, None),
            KeyBinding::new("cmd-o", OpenFile, None),
            KeyBinding::new("ctrl-o", OpenFile, None),
            KeyBinding::new("cmd-shift-o", OpenFolder, None),
            KeyBinding::new("ctrl-shift-o", OpenFolder, None),
            KeyBinding::new("cmd-s", Save, None),
            KeyBinding::new("ctrl-s", Save, None),
            KeyBinding::new("cmd-shift-s", SaveAs, None),
            KeyBinding::new("ctrl-shift-s", SaveAs, None),
            KeyBinding::new("cmd-w", CloseTab, None),
            KeyBinding::new("ctrl-w", CloseTab, None),
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("ctrl-q", Quit, None),
            KeyBinding::new("cmd-z", Undo, None),
            KeyBinding::new("ctrl-z", Undo, None),
            KeyBinding::new("cmd-shift-z", Redo, None),
            KeyBinding::new("ctrl-shift-z", Redo, None),
            KeyBinding::new("cmd-t", ToggleTheme, None),
            KeyBinding::new("ctrl-t", ToggleTheme, None),
            KeyBinding::new("cmd-,", Preferences, None),
            KeyBinding::new("ctrl-,", Preferences, None),
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
    if block.typst.is_none() {
        return false;
    }

    block.typst = Some(typst_adapter_from_result(result));

    true
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
    let mut updated = 0;
    let mut targets = Vec::new();

    for (index, block) in document.blocks().iter().enumerate() {
        // Block-level
        if matches!(block.typst, Some(TypstAdapter::Pending)) {
            if let Some((kind, source)) = typst_render_request(block) {
                if typst_cache_key(&kind, &source) == cache_key {
                    targets.push(index);
                }
            }
        }

        // Inline-level (even if not Pending, we may update it)
        if let Some(HtmlAdapter::Adapted { nodes }) = &block.html {
            for node in nodes {
                if let HtmlNode::InlineMath(source) = node {
                    if typst_cache_key(&RenderKind::Math, source) == cache_key {
                        targets.push(index);
                        break;
                    }
                }
            }
        }
    }

    for index in targets {
        if document.update_block_typst(index, adapter.clone()) {
            updated += 1;
        }
    }

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
        should_start_typst_compile, typst_adapter_from_result, typst_cache_key,
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
}

