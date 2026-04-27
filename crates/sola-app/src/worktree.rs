use gpui::{App, AppContext, Entity, EventEmitter};
use ignore::WalkBuilder;
use notify::{RecursiveMode, Watcher};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub name: String,
    pub children: Option<Vec<Entry>>,
}

pub struct Worktree {
    abs_path: PathBuf,
    root_entry: Option<Entry>,
    _watcher: Option<notify::RecommendedWatcher>,
}

pub enum WorktreeEvent {
    Updated,
}

impl EventEmitter<WorktreeEvent> for Worktree {}

impl Worktree {
    pub fn local(path: impl Into<PathBuf>, cx: &mut App) -> Entity<Self> {
        let abs_path = path
            .into()
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from("."));

        let handle = cx.new(|_| Self {
            abs_path: abs_path.clone(),
            root_entry: None,
            _watcher: None,
        });

        let weak_handle = handle.downgrade();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        // 1. Initial Background Scan
        let abs_path_initial = abs_path.clone();
        let weak_handle_initial = weak_handle.clone();
        cx.spawn(|cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let root_entry = cx
                    .background_executor()
                    .spawn(async move { scan_recursive(abs_path_initial) })
                    .await;

                let _ = weak_handle_initial.update(&mut cx, |this, cx| {
                    this.root_entry = Some(root_entry);
                    cx.emit(WorktreeEvent::Updated);
                });
            }
        })
        .detach();

        // 2. Background Watcher Setup
        let abs_path_for_watcher = abs_path.clone();
        let (watcher_tx, watcher_rx) = tokio::sync::oneshot::channel();
        let tx_for_watcher = tx.clone();

        std::thread::spawn(move || {
            let mut watcher =
                notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                    if let Ok(event) = res {
                        let is_noise = event.paths.iter().any(|p| {
                            let s = p.to_string_lossy();
                            s.contains("/target/") || s.contains("/.git/")
                        });
                        if !is_noise {
                            let _ = tx_for_watcher.send(());
                        }
                    }
                })
                .ok();

            if let Some(watcher) = &mut watcher {
                let _ = watcher.watch(&abs_path_for_watcher, RecursiveMode::Recursive);
            }

            let _ = watcher_tx.send(watcher);
        });

        let weak_handle_setup = weak_handle.clone();
        cx.spawn(|cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                if let Ok(watcher) = watcher_rx.await {
                    let _ = weak_handle_setup.update(&mut cx, |this, _| {
                        this._watcher = watcher;
                    });
                }
            }
        })
        .detach();

        // 3. Background Event Update Loop
        let weak_handle_loop = weak_handle.clone();
        let abs_path_loop = abs_path.clone();
        cx.spawn(|cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                while let Some(_) = rx.recv().await {
                    let abs_path = abs_path_loop.clone();
                    let root_entry = cx
                        .background_executor()
                        .spawn(async move { scan_recursive(abs_path) })
                        .await;

                    let _ = weak_handle_loop.update(&mut cx, |this, cx| {
                        this.root_entry = Some(root_entry);
                        cx.emit(WorktreeEvent::Updated);
                    });
                }
            }
        })
        .detach();

        handle
    }

    pub fn root_entry(&self) -> Option<&Entry> {
        self.root_entry.as_ref()
    }

    pub fn abs_path(&self) -> &Path {
        &self.abs_path
    }
}

// Helper functions for pure background scanning
fn scan_recursive(root_path: PathBuf) -> Entry {
    let mut entries_map: std::collections::HashMap<PathBuf, Vec<Entry>> =
        std::collections::HashMap::new();

    let walker = WalkBuilder::new(&root_path)
        .hidden(false)
        .git_ignore(true)
        .build();

    for result in walker {
        if let Ok(entry) = result {
            if entry.path() == root_path {
                continue;
            }

            let path = entry.path().to_path_buf();
            let is_dir = path.is_dir();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            if name == "target" || name == ".git" {
                continue;
            }

            let parent = path.parent().unwrap_or(&root_path).to_path_buf();
            entries_map.entry(parent).or_default().push(Entry {
                path,
                is_dir,
                name,
                children: if is_dir { Some(Vec::new()) } else { None },
            });
        }
    }

    build_tree_recursive(&root_path, &mut entries_map)
}

fn build_tree_recursive(
    path: &Path,
    map: &mut std::collections::HashMap<PathBuf, Vec<Entry>>,
) -> Entry {
    let mut entries = map.remove(path).unwrap_or_default();
    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            b.is_dir.cmp(&a.is_dir)
        } else {
            a.name.cmp(&b.name)
        }
    });

    for entry in &mut entries {
        if entry.is_dir {
            let mut child_node = build_tree_recursive(&entry.path, map);
            entry.children = child_node.children.take();
        }
    }

    Entry {
        path: path.to_path_buf(),
        is_dir: true,
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        children: Some(entries),
    }
}
