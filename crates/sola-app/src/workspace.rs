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

pub struct OpenDocument {
    pub path: Option<PathBuf>,
    pub document: DocumentModel,
}

pub struct Workspace {
    worktree: Entity<Worktree>,
    documents: Vec<OpenDocument>,
    active_document_index: Option<usize>,
    theme: Theme,
    theme_mode: ThemeMode,
    recent_paths: Vec<PathBuf>,
}

pub enum WorkspaceEvent {
    DocumentChanged,
    ThemeChanged,
    WorktreeChanged,
    ActiveTabChanged,
}

impl EventEmitter<WorkspaceEvent> for Workspace {}

impl Workspace {
    pub fn new(worktree: Entity<Worktree>, _cx: &mut Context<Self>) -> Self {
        Self {
            worktree,
            documents: Vec::new(),
            active_document_index: None,
            theme: Theme::sola_dark(),
            theme_mode: ThemeMode::Dark,
            recent_paths: Vec::new(),
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

    pub fn documents(&self) -> &[OpenDocument] {
        &self.documents
    }

    pub fn active_document_index(&self) -> Option<usize> {
        self.active_document_index
    }

    pub fn active_document_ref(&self) -> Option<&DocumentModel> {
        self.active_document_index
            .and_then(|idx| self.documents.get(idx))
            .map(|d| &d.document)
    }

    pub fn update_active_document<R>(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut DocumentModel) -> R,
    ) -> Option<R> {
        let idx = self.active_document_index?;
        let doc = self.documents.get_mut(idx)?;
        let result = f(&mut doc.document);
        cx.emit(WorkspaceEvent::DocumentChanged);
        cx.notify();
        Some(result)
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
        self.active_document_index
            .and_then(|idx| self.documents.get(idx))
            .and_then(|d| d.path.as_ref())
    }

    pub fn recent_paths(&self) -> &[PathBuf] {
        &self.recent_paths
    }

    pub fn clear_recent_paths(&mut self, cx: &mut Context<Self>) {
        self.recent_paths.clear();
        cx.notify();
    }

    fn add_recent_path(&mut self, path: PathBuf) {
        if let Some(pos) = self.recent_paths.iter().position(|p| p == &path) {
            self.recent_paths.remove(pos);
        }
        self.recent_paths.insert(0, path);
        if self.recent_paths.len() > 10 {
            self.recent_paths.truncate(10);
        }
    }

    pub fn open_file(&mut self, path: PathBuf, document: DocumentModel, cx: &mut Context<Self>) {
        self.add_recent_path(path.clone());
        // Check if already open
        if let Some(idx) = self.documents.iter().position(|d| d.path.as_ref() == Some(&path)) {
            self.active_document_index = Some(idx);
        } else {
            self.documents.push(OpenDocument {
                path: Some(path),
                document,
            });
            self.active_document_index = Some(self.documents.len() - 1);
        }
        cx.emit(WorkspaceEvent::DocumentChanged);
        cx.emit(WorkspaceEvent::ActiveTabChanged);
        cx.notify();
    }

    pub fn open_template(&mut self, document: DocumentModel, cx: &mut Context<Self>) {
        self.documents.push(OpenDocument {
            path: None,
            document,
        });
        self.active_document_index = Some(self.documents.len() - 1);
        cx.emit(WorkspaceEvent::DocumentChanged);
        cx.emit(WorkspaceEvent::ActiveTabChanged);
        cx.notify();
    }

    pub fn switch_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.documents.len() {
            self.active_document_index = Some(index);
            cx.emit(WorkspaceEvent::ActiveTabChanged);
            cx.emit(WorkspaceEvent::DocumentChanged);
            cx.notify();
        }
    }

    pub fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.documents.len() {
            self.documents.remove(index);
            if self.documents.is_empty() {
                self.active_document_index = None;
            } else {
                let new_idx = self.active_document_index.unwrap_or(0);
                self.active_document_index = Some(new_idx.min(self.documents.len() - 1));
            }
            cx.emit(WorkspaceEvent::ActiveTabChanged);
            cx.emit(WorkspaceEvent::DocumentChanged);
            cx.notify();
        }
    }

    pub fn save_current_file(&mut self, cx: &mut Context<Self>) {
        if let Some(idx) = self.active_document_index {
            let doc = &mut self.documents[idx];
            if let Some(path) = &doc.path {
                let content = doc.document.source();
                if let Ok(_) = std::fs::write(path, content) {
                    cx.emit(WorkspaceEvent::DocumentChanged);
                    cx.notify();
                }
            }
        }
    }

    pub fn save_as(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(idx) = self.active_document_index {
            let content = self.documents[idx].document.source().to_string();
            if let Ok(_) = std::fs::write(&path, content) {
                self.documents[idx].path = Some(path.clone());
                self.add_recent_path(path);
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
