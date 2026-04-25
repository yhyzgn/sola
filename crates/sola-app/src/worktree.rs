use gpui::{AppContext, App, AsyncApp, Entity, EventEmitter};
use std::path::{Path, PathBuf};
use notify::{Watcher, RecursiveMode};

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub name: String,
}

pub struct Worktree {
    abs_path: PathBuf,
    entries: Vec<Entry>,
    _watcher: Option<notify::RecommendedWatcher>,
}

pub enum WorktreeEvent {
    Updated,
}

impl EventEmitter<WorktreeEvent> for Worktree {}

impl Worktree {
    pub fn local(path: impl Into<PathBuf>, cx: &mut App) -> Entity<Self> {
        let abs_path = path.into().canonicalize().unwrap_or_else(|_| PathBuf::from("."));
        let mut worktree = Self {
            abs_path: abs_path.clone(),
            entries: Vec::new(),
            _watcher: None,
        };

        worktree.scan();
        
        let (tx, rx) = std::sync::mpsc::channel();
        
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Ok(_) = res {
                let _ = tx.send(());
            }
        }).ok();

        if let Some(watcher) = &mut watcher {
            let _ = watcher.watch(&abs_path, RecursiveMode::Recursive);
        }

        worktree._watcher = watcher;

        let handle = cx.new(|_| worktree);
        let weak_handle = handle.downgrade();

        cx.spawn(|cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                while let Ok(_) = rx.recv() {
                    if let Err(_) = weak_handle.update(&mut cx, |this, cx| {
                        this.scan();
                        cx.emit(WorktreeEvent::Updated);
                    }) {
                        break;
                    }
                }
            }
        }).detach();

        handle
    }

    fn scan(&mut self) {
        let mut new_entries = Vec::new();
        if let Ok(read_dir) = std::fs::read_dir(&self.abs_path) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                new_entries.push(Entry {
                    is_dir: path.is_dir(),
                    path,
                    name,
                });
            }
        }
        new_entries.sort_by(|a, b| {
            if a.is_dir != b.is_dir {
                b.is_dir.cmp(&a.is_dir)
            } else {
                a.name.cmp(&b.name)
            }
        });
        self.entries = new_entries;
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn abs_path(&self) -> &Path {
        &self.abs_path
    }
}
