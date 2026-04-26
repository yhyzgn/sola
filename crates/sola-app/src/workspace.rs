use gpui::{Context, Entity, EventEmitter};
use crate::worktree::Worktree;
use sola_document::DocumentModel;
use sola_theme::Theme;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    pub fn toggle(&self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

pub struct Workspace {
    worktree: Entity<Worktree>,
    document: DocumentModel,
    theme: Theme,
    theme_mode: ThemeMode,
    current_path: Option<PathBuf>,
}

pub enum WorkspaceEvent {
    DocumentChanged,
    ThemeChanged,
    WorktreeChanged,
}

impl EventEmitter<WorkspaceEvent> for Workspace {}

impl Workspace {
    pub fn new(worktree: Entity<Worktree>, _cx: &mut Context<Self>) -> Self {
        Self {
            worktree,
            document: DocumentModel::from_markdown(""),
            theme: Theme::sola_dark(),
            theme_mode: ThemeMode::Dark,
            current_path: None,
        }
    }

    pub fn worktree(&self) -> &Entity<Worktree> {
        &self.worktree
    }

    pub fn update_worktree(&mut self, worktree: Entity<Worktree>, cx: &mut Context<Self>) {
        self.worktree = worktree;
        cx.emit(WorkspaceEvent::WorktreeChanged);
        cx.notify();
    }

    pub fn document(&self) -> &DocumentModel {
        &self.document
    }

    pub fn document_mut(&mut self) -> &mut DocumentModel {
        &mut self.document
    }

    pub fn update_document<R>(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut DocumentModel) -> R,
    ) -> R {
        let result = f(&mut self.document);
        cx.emit(WorkspaceEvent::DocumentChanged);
        cx.notify();
        result
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn theme_mode(&self) -> ThemeMode {
        self.theme_mode
    }

    pub fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        self.theme_mode = self.theme_mode.toggle();
        self.theme = match self.theme_mode {
            ThemeMode::Dark => Theme::sola_dark(),
            ThemeMode::Light => Theme::sola_light(),
        };
        cx.emit(WorkspaceEvent::ThemeChanged);
        cx.notify();
    }

    pub fn current_path(&self) -> Option<&PathBuf> {
        self.current_path.as_ref()
    }

    pub fn open_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            self.document = DocumentModel::from_markdown(&content);
            self.current_path = Some(path);
            cx.emit(WorkspaceEvent::DocumentChanged);
            cx.notify();
        }
    }

    pub fn save_current_file(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = &self.current_path {
            let content = self.document.source();
            if let Ok(_) = std::fs::write(path, content) {
                // We could emit a specific 'Saved' event, but for now just notify
                cx.emit(WorkspaceEvent::DocumentChanged);
                cx.notify();
            }
        }
    }

    pub fn create_file(&mut self, parent_dir: PathBuf, name: &str) {
        let path = parent_dir.join(name);
        let _ = std::fs::write(path, "");
    }

    pub fn create_dir(&mut self, parent_dir: PathBuf, name: &str) {
        let path = parent_dir.join(name);
        let _ = std::fs::create_dir_all(path);
    }

    pub fn rename_entry(&mut self, old_path: PathBuf, new_path: PathBuf) {
        let _ = std::fs::rename(old_path, new_path);
    }

    pub fn delete_entry(&mut self, path: PathBuf) {
        if path.is_dir() {
            let _ = std::fs::remove_dir_all(path);
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
}
