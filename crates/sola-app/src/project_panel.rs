use crate::workspace::{Workspace, WorkspaceEvent};
use crate::worktree::{Entry, WorktreeEvent};
use gpui::prelude::*;
use gpui::{
    div, px, App, Context, Div, Entity, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Point, Pixels, Render, Styled, WeakEntity,
};
use std::collections::HashSet;
use std::path::PathBuf;

pub struct ProjectPanel {
    workspace: Entity<Workspace>,
    expanded_dirs: HashSet<PathBuf>,
    this_handle: Option<WeakEntity<Self>>,
    context_menu: Option<ContextMenuState>,
}

struct ContextMenuState {
    path: PathBuf,
    position: Point<Pixels>,
}

impl ProjectPanel {
    pub fn new(workspace: Entity<Workspace>, _cx: &mut Context<Self>) -> Self {
        Self {
            workspace,
            expanded_dirs: HashSet::new(),
            this_handle: None,
            context_menu: None,
        }
    }

    pub fn set_handle(&mut self, handle: WeakEntity<Self>, cx: &mut Context<Self>) {
        self.this_handle = Some(handle);
        
        cx.subscribe(&self.workspace, |this, _workspace, event, cx| match event {
            WorkspaceEvent::DocumentChanged | WorkspaceEvent::ThemeChanged => {
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
        self.workspace.update(cx, |workspace, cx| {
            workspace.open_file(path, cx);
        });
    }

    fn show_context_menu(&mut self, path: PathBuf, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.context_menu = Some(ContextMenuState { path, position });
        cx.notify();
    }

    fn hide_context_menu(&mut self, cx: &mut Context<Self>) {
        self.context_menu = None;
        cx.notify();
    }

    fn render_context_menu(&self, menu: &ContextMenuState, cx: &mut Context<Self>) -> Div {
        let theme = self.workspace.read(cx).theme().clone();
        let path = menu.path.clone();

        let new_file_path = path.clone();
        let new_folder_path = path.clone();
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
            .child(self.render_menu_item("New File", move |this, cx| {
                this.workspace.update(cx, |workspace, _cx| {
                    workspace.create_file(new_file_path.clone(), "untitled.md");
                });
            }, cx))
            .child(self.render_menu_item("New Folder", move |this, cx| {
                this.workspace.update(cx, |workspace, _cx| {
                    workspace.create_dir(new_folder_path.clone(), "new_folder");
                });
            }, cx))
            .child(div().h(px(1.0)).bg(crate::shell::rgb_hex(&theme.palette.panel_border)).my(px(4.0)))
            .child(self.render_menu_item("Delete", move |this, cx| {
                this.workspace.update(cx, |workspace, _cx| {
                    workspace.delete_entry(delete_path.clone());
                });
            }, cx))
    }

    fn render_menu_item(
        &self,
        label: &'static str,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + Send + Sync + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(4.0))
            .hover(|s| s.bg(crate::shell::rgb_hex("#3a3a3a"))) // Simple hover for now
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _event, _window, cx| {
                on_click(this, cx);
                this.hide_context_menu(cx);
            }))
            .child(
                div()
                    .text_size(px(13.0))
                    .child(label)
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
            .bg(crate::shell::rgb_hex(&theme.palette.panel_background))
            .border_r_1()
            .border_color(crate::shell::rgb_hex(&theme.palette.panel_border))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _event, _window, cx| {
                this.hide_context_menu(cx);
            }))
            .child(
                div()
                    .p(px(16.0))
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(crate::shell::rgb_hex(&theme.palette.text_primary))
                    .child("PROJECT"),
            )
            .child(
                div()
                    .flex_1()
                    .child(
                        if let Some(this_handle) = this_handle {
                            div().flex_1().child(
                                gpui::list(
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
                                            .on_mouse_down(MouseButton::Right, move |event: &gpui::MouseDownEvent, _window, cx| {
                                                let path = right_click_path.clone();
                                                let pos = event.position;
                                                let _ = right_handle.update(cx, |this, cx| {
                                                    this.show_context_menu(path, pos, cx);
                                                });
                                            })
                                            .child(
                                                div()
                                                    .flex()
                                                    .gap(px(6.0))
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_size(px(12.0))
                                                            .child(if is_dir { if is_expanded { "▼" } else { "▶" } } else { "📄" })
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(13.0))
                                                            .child(name)
                                                    )
                                            )
                                            .into_any_element()
                                    }
                                )
                            )
                        } else {
                            div()
                        }
                    )
            )
            .when_some(self.context_menu.as_ref(), |this, menu| {
                this.child(self.render_context_menu(menu, cx))
            })
    }
}
