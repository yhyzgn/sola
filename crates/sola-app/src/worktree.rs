use gpui::{AppContext, App, Entity, EventEmitter};
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

        let handle = cx.new(|_| Self {
            abs_path: abs_path.clone(),
            entries: Vec::new(),
            _watcher: None,
        });

        handle.update(cx, |this, _| this.scan());

        let weak_handle = handle.downgrade();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let weak_handle_for_setup = weak_handle.clone();
        let abs_path_for_watcher = abs_path.clone();
        let (watcher_tx, watcher_rx) = tokio::sync::oneshot::channel();
        let tx_for_watcher = tx.clone();
        
        std::thread::spawn(move || {
            let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    let is_noise = event.paths.iter().any(|p| {
                        let s = p.to_string_lossy();
                        s.contains("/target/") || s.contains("/.git/")
                    });
                    if !is_noise {
                        let _ = tx_for_watcher.send(());
                    }
                }
            }).ok();

            if let Some(watcher) = &mut watcher {
                let _ = watcher.watch(&abs_path_for_watcher, RecursiveMode::Recursive);
            }
            
            let _ = watcher_tx.send(watcher);
        });

        cx.spawn(|cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                if let Ok(watcher) = watcher_rx.await {
                    let _ = weak_handle_for_setup.update(&mut cx, |this, _| {
                        this._watcher = watcher;
                    });
                }
            }
        }).detach();

        cx.spawn(|cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                while let Some(_) = rx.recv().await {
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
