use crate::workspace::{Workspace, WorkspaceEvent};
use crate::worktree::{Entry, WorktreeEvent};
use gpui::prelude::*;
use gpui::{
    App, Context, Div, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
    Point, Render, Styled, WeakEntity, div, px,
};
use sola_document::DocumentModel;
use std::collections::HashSet;
use std::path::PathBuf;

pub struct ProjectPanel {
    workspace: Entity<Workspace>,
    expanded_dirs: HashSet<PathBuf>,
    this_handle: Option<WeakEntity<Self>>,
    context_menu: Option<ContextMenuState>,
    input_state: Option<InputState>,
    mode: SidebarMode,
}

struct ContextMenuState {
    path: PathBuf,
    position: Point<Pixels>,
}

enum InputAction {
    CreateFile(PathBuf),
    CreateDir(PathBuf),
    Rename(PathBuf),
}

struct InputState {
    action: InputAction,
    value: String,
    focus_handle: gpui::FocusHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidebarMode {
    Files,
    Outline,
}

impl ProjectPanel {
    pub fn new(workspace: Entity<Workspace>, _cx: &mut Context<Self>) -> Self {
        Self {
            workspace,
            expanded_dirs: HashSet::new(),
            this_handle: None,
            context_menu: None,
            input_state: None,
            mode: SidebarMode::Files,
        }
    }

    pub fn set_handle(&mut self, handle: WeakEntity<Self>, cx: &mut Context<Self>) {
        self.this_handle = Some(handle);

        cx.subscribe(&self.workspace, |this, _workspace, event, cx| match event {
            WorkspaceEvent::DocumentChanged
            | WorkspaceEvent::ThemeChanged
            | WorkspaceEvent::ActiveTabChanged => {
                cx.notify();
            }
            WorkspaceEvent::WorktreeChanged => {
                this.subscribe_to_worktree(cx);
                cx.notify();
            }
        })
        .detach();

        self.subscribe_to_worktree(cx);
    }

    fn subscribe_to_worktree(&mut self, cx: &mut Context<Self>) {
        let worktree = self.workspace.read(cx).worktree().clone();
        cx.subscribe(&worktree, |_this, _worktree, event, cx| match event {
            WorktreeEvent::Updated => {
                cx.notify();
            }
        })
        .detach();
    }

    fn toggle_directory(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.expanded_dirs.contains(&path) {
            self.expanded_dirs.remove(&path);
        } else {
            self.expanded_dirs.insert(path);
        }
        cx.notify();
    }

    fn open_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
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

    fn show_context_menu(
        &mut self,
        path: PathBuf,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = Some(ContextMenuState { path, position });
        cx.notify();
    }

    fn hide_context_menu(&mut self, cx: &mut Context<Self>) {
        self.context_menu = None;
        cx.notify();
    }

    fn start_input(
        &mut self,
        action: InputAction,
        initial_value: String,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);
        self.input_state = Some(InputState {
            action,
            value: initial_value,
            focus_handle,
        });
        self.context_menu = None;
        cx.notify();
    }

    fn finish_input(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.input_state.take() {
            let value = state.value.trim();
            if !value.is_empty() {
                match state.action {
                    InputAction::CreateFile(parent) => {
                        self.workspace
                            .update(cx, |w, _cx| w.create_file(parent, value));
                    }
                    InputAction::CreateDir(parent) => {
                        self.workspace
                            .update(cx, |w, _cx| w.create_dir(parent, value));
                    }
                    InputAction::Rename(old_path) => {
                        let mut new_path = old_path.clone();
                        new_path.set_file_name(value);
                        self.workspace
                            .update(cx, |w, _cx| w.rename_entry(old_path, new_path));
                    }
                }
            }
        }
        cx.notify();
    }

    fn cancel_input(&mut self, cx: &mut Context<Self>) {
        self.input_state = None;
        cx.notify();
    }

    fn render_context_menu(&self, menu: &ContextMenuState, cx: &mut Context<Self>) -> Div {
        let theme = self.workspace.read(cx).theme().clone();
        let path = menu.path.clone();

        let parent_path = if path.is_dir() {
            path.clone()
        } else {
            path.parent().unwrap_or(&path).to_path_buf()
        };

        let new_file_path = parent_path.clone();
        let new_folder_path = parent_path.clone();
        let rename_path = path.clone();
        let delete_path = path.clone();

        div()
            .absolute()
            .top(menu.position.y)
            .left(menu.position.x)
            .bg(crate::shell::rgb_hex(&theme.palette.panel_background))
            .border_1()
            .border_color(crate::shell::rgb_hex(&theme.palette.panel_border))
            .rounded(px(8.0))
            .p(px(4.0))
            .min_w(px(160.0))
            .flex()
            .flex_col()
            .child(self.render_menu_item(
                "New File",
                move |this, window, cx| {
                    this.start_input(
                        InputAction::CreateFile(new_file_path.clone()),
                        "untitled.md".to_string(),
                        window,
                        cx,
                    );
                },
                cx,
            ))
            .child(self.render_menu_item(
                "New Folder",
                move |this, window, cx| {
                    this.start_input(
                        InputAction::CreateDir(new_folder_path.clone()),
                        "new_folder".to_string(),
                        window,
                        cx,
                    );
                },
                cx,
            ))
            .child(self.render_menu_item(
                "Rename",
                move |this, window, cx| {
                    let name = rename_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    this.start_input(InputAction::Rename(rename_path.clone()), name, window, cx);
                },
                cx,
            ))
            .child(
                div()
                    .h(px(1.0))
                    .bg(crate::shell::rgb_hex(&theme.palette.panel_border))
                    .my(px(4.0)),
            )
            .child(self.render_menu_item(
                "Delete",
                move |this, _window, cx| {
                    this.workspace.update(cx, |workspace, _cx| {
                        workspace.delete_entry(delete_path.clone());
                    });
                },
                cx,
            ))
    }

    fn render_menu_item(
        &self,
        label: &'static str,
        on_click: impl Fn(&mut Self, &mut gpui::Window, &mut Context<Self>) + Send + Sync + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(4.0))
            .hover(|s| s.bg(crate::shell::rgb_hex("#3a3a3a")))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    on_click(this, window, cx);
                    this.hide_context_menu(cx);
                }),
            )
            .child(div().text_size(px(13.0)).child(label))
    }

    fn render_input_modal(&self, state: &InputState, cx: &mut Context<Self>) -> Div {
        let theme = self.workspace.read(cx).theme().clone();
        let prompt = match state.action {
            InputAction::CreateFile(_) => "New File Name:",
            InputAction::CreateDir(_) => "New Folder Name:",
            InputAction::Rename(_) => "Rename to:",
        };

        div()
            .absolute()
            .size_full()
            .track_focus(&state.focus_handle)
            .bg(gpui::hsla(0.0, 0.0, 0.0, 0.5)) // Semi-transparent mask
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.cancel_input(cx);
                }),
            )
            .child(
                div()
                    .w(px(400.0))
                    .bg(crate::shell::rgb_hex(&theme.palette.panel_background))
                    .border_1()
                    .border_color(crate::shell::rgb_hex(&theme.palette.panel_border))
                    .rounded(px(12.0))
                    .p(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .on_mouse_down(MouseButton::Left, |_, _, _| {}) // Prevent click-through
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(crate::shell::rgb_hex(&theme.palette.text_primary))
                            .child(prompt),
                    )
                    .child(
                        div()
                            .p(px(12.0))
                            .bg(crate::shell::rgb_hex(&theme.palette.code_background))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(crate::shell::rgb_hex(&theme.palette.focused_border))
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(crate::shell::rgb_hex(&theme.palette.text_primary))
                                    .child(state.value.clone()),
                            )
                            .child(
                                // Blinking cursor
                                div()
                                    .w(px(2.0))
                                    .h(px(16.0))
                                    .bg(crate::shell::rgb_hex(&theme.palette.accent))
                                    .ml(px(2.0)),
                            ),
                    )
                    .on_key_down(cx.listener(
                        move |this, event: &gpui::KeyDownEvent, _window, cx| {
                            let key = event.keystroke.key.as_str();
                            if let Some(state) = &mut this.input_state {
                                match key {
                                    "enter" => this.finish_input(cx),
                                    "escape" => this.cancel_input(cx),
                                    "backspace" => {
                                        state.value.pop();
                                        cx.notify();
                                    }
                                    _ => {
                                        if let Some(ch) = &event.keystroke.key_char {
                                            state.value.push_str(ch);
                                            cx.notify();
                                        }
                                    }
                                }
                            }
                        },
                    )),
            )
    }

    fn visible_entries(&self, cx: &App) -> Vec<(usize, Entry)> {
        let workspace = self.workspace.read(cx);
        let worktree = workspace.worktree().read(cx);
        let mut visible = Vec::new();
        if let Some(root) = worktree.root_entry() {
            if let Some(children) = &root.children {
                self.collect_visible(children, 0, &mut visible);
            }
        }
        visible
    }

    fn collect_visible(&self, entries: &[Entry], depth: usize, out: &mut Vec<(usize, Entry)>) {
        for entry in entries {
            out.push((depth, entry.clone()));
            if entry.is_dir && self.expanded_dirs.contains(&entry.path) {
                if let Some(children) = &entry.children {
                    self.collect_visible(children, depth + 1, out);
                }
            }
        }
    }

    fn render_mode_toggle(&self, cx: &mut Context<Self>) -> Div {
        let theme = self.workspace.read(cx).theme();
        let mode = self.mode;

        div()
            .flex()
            .bg(crate::shell::rgb_hex(&theme.palette.panel_background))
            .border_b_1()
            .border_color(crate::shell::rgb_hex(&theme.palette.panel_border))
            .child(self.render_mode_button(
                "FILES",
                mode == SidebarMode::Files,
                SidebarMode::Files,
                cx,
            ))
            .child(self.render_mode_button(
                "OUTLINE",
                mode == SidebarMode::Outline,
                SidebarMode::Outline,
                cx,
            ))
    }

    fn render_mode_button(
        &self,
        label: &'static str,
        is_active: bool,
        mode: SidebarMode,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.workspace.read(cx).theme();

        div()
            .flex_1()
            .py(px(10.0))
            .flex()
            .justify_center()
            .cursor_pointer()
            .border_b_1()
            .border_color(if is_active {
                crate::shell::rgb_hex(&theme.palette.accent)
            } else {
                gpui::hsla(0.0, 0.0, 0.0, 0.0)
            })
            .hover(|s| s.bg(crate::shell::rgb_hex(&theme.palette.code_background)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.mode = mode;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(if is_active {
                        crate::shell::rgb_hex(&theme.palette.text_primary)
                    } else {
                        crate::shell::rgb_hex(&theme.palette.text_muted)
                    })
                    .child(label),
            )
    }

    fn render_outline(&self, cx: &mut Context<Self>) -> Div {
        let workspace = self.workspace.read(cx);
        let Some(document) = workspace.active_document_ref() else {
            return div()
                .p(px(16.0))
                .text_color(crate::shell::rgb_hex("#888888"))
                .child("No document open");
        };

        let headings = document.get_headings();
        if headings.is_empty() {
            return div()
                .p(px(16.0))
                .text_color(crate::shell::rgb_hex("#888888"))
                .child("No headings found");
        }

        let theme = workspace.theme().clone();
        let this_handle = self.this_handle.clone();

        div().flex_1().child(gpui::list(
            gpui::ListState::new(headings.len(), gpui::ListAlignment::Top, px(100.0)),
            move |idx, _window, _cx| {
                let (block_index, level, title) = &headings[idx];
                let block_index = *block_index;
                let level = *level;
                let title = title.clone();

                let handle = this_handle.clone();

                div()
                    .px(px(16.0))
                    .pl(px(16.0 + (level.saturating_sub(1) as f32) * 12.0))
                    .py(px(6.0))
                    .hover(|s| s.bg(crate::shell::rgb_hex("#3a3a3a")))
                    .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                        if let Some(handle) = &handle {
                            let _ = handle.update(cx, |this, cx| {
                                this.workspace.update(cx, |workspace, cx| {
                                    workspace.update_active_document(cx, |doc| {
                                        doc.focus_block(block_index);
                                    });
                                });
                            });
                        }
                    })
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(crate::shell::rgb_hex(if level == 1 {
                                &theme.palette.text_primary
                            } else {
                                &theme.palette.text_muted
                            }))
                            .child(title),
                    )
                    .into_any_element()
            },
        ))
    }
}

impl Render for ProjectPanel {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.workspace.read(cx).theme().clone();
        let entries = self.visible_entries(cx);
        let expanded_dirs = self.expanded_dirs.clone();
        let this_handle = self.this_handle.clone();

        div()
            .w(px(300.0))
            .h_full()
            .relative()
            .bg(crate::shell::rgb_hex(&theme.palette.panel_background))
            .border_r_1()
            .border_color(crate::shell::rgb_hex(&theme.palette.panel_border))
            .flex()
            .flex_col()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.hide_context_menu(cx);
                }),
            )
            .child(self.render_mode_toggle(cx))
            .child(div().flex_1().child(match self.mode {
                SidebarMode::Files => {
                    if let Some(this_handle) = this_handle {
                        div().flex_1().child(gpui::list(
                            gpui::ListState::new(
                                entries.len(),
                                gpui::ListAlignment::Top,
                                px(100.0),
                            ),
                            move |idx, _window, _cx| {
                                let (depth, entry) = &entries[idx];
                                let is_expanded = expanded_dirs.contains(&entry.path);
                                let is_dir = entry.is_dir;
                                let name = entry.name.clone();
                                let path = entry.path.clone();

                                let left_click_path = path.clone();
                                let right_click_path = path.clone();
                                let left_handle = this_handle.clone();
                                let right_handle = this_handle.clone();

                                div()
                                    .px(px(16.0))
                                    .pl(px(16.0 + (*depth as f32) * 12.0))
                                    .py(px(4.0))
                                    .hover(|s| s.bg(crate::shell::rgb_hex("#3a3a3a")))
                                    .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                                        let path = left_click_path.clone();
                                        let _ = left_handle.update(cx, |this, cx| {
                                            if is_dir {
                                                this.toggle_directory(path, cx);
                                            } else {
                                                this.open_file(path, cx);
                                            }
                                        });
                                    })
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        move |event: &gpui::MouseDownEvent, _window, cx| {
                                            let path = right_click_path.clone();
                                            let pos = event.position;
                                            let _ = right_handle.update(cx, |this, cx| {
                                                this.show_context_menu(path, pos, cx);
                                            });
                                        },
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(6.0))
                                            .items_center()
                                            .child(div().text_size(px(12.0)).child(if is_dir {
                                                if is_expanded { "▼" } else { "▶" }
                                            } else {
                                                "📄"
                                            }))
                                            .child(div().text_size(px(13.0)).child(name)),
                                    )
                                    .into_any_element()
                            },
                        ))
                    } else {
                        div()
                    }
                }
                SidebarMode::Outline => self.render_outline(cx),
            }))
            .when_some(self.context_menu.as_ref(), |this, menu| {
                this.child(self.render_context_menu(menu, cx))
            })
            .when_some(self.input_state.as_ref(), |this, state| {
                this.child(self.render_input_modal(state, cx))
            })
    }
}
