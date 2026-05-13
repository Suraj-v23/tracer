use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;

use crate::graph::store::{GraphNode, Store};

// ─── Stats ────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct IndexStats {
    pub total:    usize,
    pub indexed:  usize,
    pub errors:   usize,
    pub watching: bool,
}

// ─── Scan ─────────────────────────────────────────────────────────────────────

/// Phase 1: collect all file metadata from disk — no store lock needed.
/// Returns (dir_entries, graph_nodes). Can take seconds; caller must NOT hold
/// the store mutex while calling this.
pub fn collect_nodes(root: &Path) -> (Vec<walkdir::DirEntry>, Vec<GraphNode>) {
    let entries: Vec<_> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| !e.path().symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(true))
        .collect();

    let nodes: Vec<GraphNode> = entries.par_iter()
        .filter_map(|entry| entry_to_node(entry.path()))
        .collect();

    (entries, nodes)
}

/// Phase 2: write collected nodes into the store. Caller controls lock granularity.
pub fn insert_nodes(store: &Store, entries: &[walkdir::DirEntry], nodes: &[GraphNode]) -> IndexStats {
    let total = nodes.len();
    let mut indexed = 0;
    let mut errors  = 0;

    for node in nodes {
        match store.upsert_node(node) {
            Ok(_)  => indexed += 1,
            Err(_) => errors  += 1,
        }
    }

    for entry in entries {
        if let Some(parent) = entry.path().parent() {
            let _ = store.upsert_edge(
                &entry.path().to_string_lossy(),
                &parent.to_string_lossy(),
                "parent",
            );
        }
    }

    insert_duplicate_edges(store, nodes);

    IndexStats { total, indexed, errors, watching: false }
}

pub fn scan_and_index(root: &Path, store: &Store) -> Result<IndexStats, String> {
    let (entries, nodes) = collect_nodes(root);
    Ok(insert_nodes(store, &entries, &nodes))
}

pub fn entry_to_node(path: &Path) -> Option<GraphNode> {
    let meta = path.symlink_metadata().ok()?;
    if meta.file_type().is_symlink() { return None; }

    let is_dir = meta.is_dir();
    let name   = path.file_name()?.to_string_lossy().to_string();
    let size   = if is_dir { 0 } else { meta.len() };

    let extension = if !is_dir {
        path.extension().map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
    } else { None };

    let to_secs = |t: std::time::SystemTime| -> Option<i64> {
        t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
    };
    let modified_secs = meta.modified().ok().and_then(to_secs);
    let created_secs  = meta.created().ok().and_then(to_secs);

    let content_hash = if !is_dir && size < 50 * 1024 * 1024 {
        hash_file(path)
    } else { None };

    Some(GraphNode {
        id: 0,
        path: path.to_string_lossy().to_string(),
        name,
        kind: if is_dir { "directory".into() } else { "file".into() },
        size,
        extension,
        modified_secs,
        created_secs,
        content_hash,
    })
}

fn hash_file(path: &Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&data);
    Some(hasher.finalize().to_hex().to_string())
}

pub fn insert_duplicate_edges_pub(store: &Store, nodes: &[GraphNode]) {
    insert_duplicate_edges(store, nodes);
}

fn insert_duplicate_edges(store: &Store, nodes: &[GraphNode]) {
    let mut by_hash: HashMap<&str, Vec<&str>> = HashMap::new();
    for node in nodes {
        if let Some(hash) = &node.content_hash {
            by_hash.entry(hash.as_str()).or_default().push(&node.path);
        }
    }
    for paths in by_hash.values().filter(|v| v.len() > 1) {
        for i in 0..paths.len() {
            for j in (i + 1)..paths.len() {
                let _ = store.upsert_edge(paths[i], paths[j], "duplicate");
                let _ = store.upsert_edge(paths[j], paths[i], "duplicate");
            }
        }
    }
}

// ─── File Watcher ─────────────────────────────────────────────────────────────

pub fn start_watcher(
    root: String,
    store: Arc<Mutex<Store>>,
    app: tauri::AppHandle,
) {
    use notify_debouncer_mini::{new_debouncer, notify::{RecursiveMode, Watcher}, DebounceEventResult};
    use std::time::Duration;
    use tauri::Emitter;

    std::thread::spawn(move || {
        let store_ref = store.clone();
        let app_ref   = app.clone();

        let mut debouncer = match new_debouncer(
            Duration::from_millis(500),
            move |result: DebounceEventResult| {
                if let Ok(events) = result {
                    for event in events {
                        handle_event(&event, &store_ref, &app_ref);
                    }
                }
            },
        ) {
            Ok(d)  => d,
            Err(e) => { eprintln!("[graph watcher] init failed: {e}"); return; }
        };

        if let Err(e) = debouncer.watcher().watch(Path::new(&root), RecursiveMode::Recursive) {
            eprintln!("[graph watcher] watch failed: {e}");
            return;
        }

        loop { std::thread::sleep(Duration::from_secs(60)); }
    });
}

fn handle_event(
    event: &notify_debouncer_mini::DebouncedEvent,
    store: &Arc<Mutex<Store>>,
    app: &tauri::AppHandle,
) {
    use tauri::Emitter;

    let path = &event.path;

    // If path still exists on disk, upsert it; otherwise remove it from the graph.
    if path.exists() {
        if let Some(node) = entry_to_node(path) {
            if let Ok(s) = store.lock() {
                let _ = s.upsert_node(&node);
                if let Some(parent) = path.parent() {
                    let _ = s.upsert_edge(
                        &path.to_string_lossy(),
                        &parent.to_string_lossy(),
                        "parent",
                    );
                }
            }
        }
    } else {
        if let Ok(s) = store.lock() {
            let _ = s.delete_node(&path.to_string_lossy());
        }
    }

    app.emit("graph-updated", ()).ok();
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::store::Store;
    use std::fs;

    fn temp_tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"hello world").unwrap();
        fs::write(dir.path().join("b.txt"), b"hello world").unwrap();
        fs::write(dir.path().join("c.rs"),  b"fn main() {}").unwrap();
        let sub = dir.path().join("src");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("lib.rs"), b"pub fn foo() {}").unwrap();
        dir
    }

    #[test]
    fn scan_indexes_all_files() {
        let dir   = temp_tree();
        let store = Store::open_in_memory().unwrap();
        let stats = scan_and_index(dir.path(), &store).unwrap();
        assert!(stats.indexed >= 5, "expected >=5 indexed, got {}", stats.indexed);
        assert_eq!(stats.errors, 0);
    }

    #[test]
    fn scan_detects_duplicates() {
        let dir   = temp_tree();
        let store = Store::open_in_memory().unwrap();
        scan_and_index(dir.path(), &store).unwrap();

        let a_path = dir.path().join("a.txt").to_string_lossy().to_string();
        let dupes  = store.find_duplicates(&a_path).unwrap();
        assert_eq!(dupes.len(), 1, "a.txt should have 1 duplicate (b.txt)");
        assert_eq!(dupes[0].name, "b.txt");
    }
}
