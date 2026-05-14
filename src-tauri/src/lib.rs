use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
mod transfer;
mod graph;
use transfer::commands::TransferAppState;
use tauri::Manager;

fn physical_size(m: &std::fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        m.blocks() * 512
    }
    #[cfg(not(unix))]
    {
        m.len()
    }
}

fn is_reparse_point(m: &std::fs::Metadata) -> bool {
    if m.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

// ─── Data types ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
struct FsNode {
    name: String,
    path: String,
    #[serde(rename = "type")]
    node_type: String,
    size: u64,
    size_human: String,
    modified_time: String,
    created_time: String,
    readonly: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    extension: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    children_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    children: Option<Vec<FsNode>>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = bytes as f64;
    let mut idx = 0;
    while size >= 1000.0 && idx < units.len() - 1 {
        size /= 1000.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{} {}", bytes, units[idx])
    } else {
        format!("{:.1} {}", size, units[idx])
    }
}

/// Physical block size — accurate on APFS, avoids double-counting clones/sparse files.
fn get_dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .same_file_system(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            !e.file_type().is_dir()
                && e.metadata().map(|m| !is_reparse_point(&m)).unwrap_or(false)
        })
        .map(|e| e.metadata().map(|m| physical_size(&m)).unwrap_or(0))
        .sum()
}

/// Parallel directory scan. Each depth level fans out across rayon's thread pool,
/// so sibling `get_dir_size` calls run concurrently instead of serially.
/// Sizes bubble up from scanned children — only leaf dirs (at max_depth boundary)
/// need a fresh `get_dir_size` walk.
fn scan_dir(path: &Path, current_depth: usize, max_depth: usize) -> Option<Vec<FsNode>> {
    if current_depth >= max_depth {
        return None;
    }

    let entries: Vec<_> = std::fs::read_dir(path).ok()?.flatten().collect();

    let mut nodes: Vec<FsNode> = entries
        .par_iter()
        .filter_map(|entry| {
            let metadata = entry.path().symlink_metadata().ok()?;
            if is_reparse_point(&metadata) {
                return None;
            }

            let is_dir = metadata.is_dir();
            let ep = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let extension = if !is_dir {
                ep.extension()
                    .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
            } else {
                None
            };
            let readonly = metadata.permissions().readonly();
            let fmt_time = |t: std::time::SystemTime| {
                chrono::DateTime::<chrono::Local>::from(t)
                    .format("%d %b %Y, %H:%M")
                    .to_string()
            };
            let modified_time = metadata.modified().map(fmt_time)
                .unwrap_or_else(|_| "—".into());
            let created_time = metadata.created().map(fmt_time)
                .unwrap_or_else(|_| "—".into());

            // Recurse first so we can bubble sizes up and skip re-walking.
            let children = if is_dir {
                scan_dir(&ep, current_depth + 1, max_depth)
            } else {
                None
            };

            let children_count = if is_dir {
                Some(match &children {
                    Some(ch) => ch.len(),
                    // Leaf dir at max_depth — quick count without full walk
                    None => std::fs::read_dir(&ep).map(|d| d.count()).unwrap_or(0),
                })
            } else {
                None
            };

            let size = if is_dir {
                match &children {
                    Some(ch) => ch.iter().map(|c| c.size).sum(),
                    None => get_dir_size(&ep),
                }
            } else {
                physical_size(&metadata)
            };

            Some(FsNode {
                name,
                path: ep.to_string_lossy().to_string(),
                node_type: if is_dir { "directory".into() } else { "file".into() },
                size,
                size_human: format_size(size),
                modified_time,
                created_time,
                readonly,
                extension,
                children_count,
                children,
            })
        })
        .collect();

    nodes.sort_by_key(|b| std::cmp::Reverse(b.size));
    Some(nodes)
}

use std::sync::{Arc, Mutex, OnceLock};
use std::collections::HashMap;
use std::time::{Instant, Duration};

static FS_CACHE: OnceLock<Mutex<HashMap<String, (Instant, FsNode)>>> = OnceLock::new();

const CACHE_TTL: Duration = Duration::from_secs(30);

fn get_cache() -> &'static Mutex<HashMap<String, (Instant, FsNode)>> {
    FS_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

/// Scans a directory tree and returns it as a JSON-serialisable FsNode tree.
/// Runs the blocking scan on a dedicated thread so the Tauri event loop stays responsive.
#[tauri::command]
async fn get_filesystem(path: Option<String>, depth: Option<usize>, force: Option<bool>) -> Result<FsNode, String> {
    let home: PathBuf = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let req = path.unwrap_or_else(|| home.to_string_lossy().to_string());
    let depth = depth.unwrap_or(2);
    let force_refresh = force.unwrap_or(false);

    let cache_key = format!("{}::{}", req, depth);

    // Check cache (fast, no blocking)
    if let Ok(mut cache) = get_cache().lock() {
        if !force_refresh {
            if let Some((ts, node)) = cache.get(&cache_key) {
                if ts.elapsed() < CACHE_TTL {
                    return Ok(node.clone());
                }
            }
        }
        if cache.len() > 100 {
            cache.retain(|_, (ts, _)| ts.elapsed() < CACHE_TTL);
        }
    }

    // Heavy scan on blocking thread pool
    let node = tauri::async_runtime::spawn_blocking(move || -> Result<FsNode, String> {
        let p = PathBuf::from(&req);
        if !p.exists() {
            return Err(format!("Path not found: {req}"));
        }

        let children = match scan_dir(&p, 0, depth) {
            Some(c) => c,
            None => match std::fs::read_dir(&p) {
                Err(e) => return Err(format!("Cannot read directory: {e}")),
                Ok(_)  => vec![],
            },
        };
        // Root size is sum of children — avoids an extra full-tree walk.
        let size: u64 = children.iter().map(|c| c.size).sum();

        let meta = p.metadata().ok();
        let readonly = meta.as_ref().map(|m| m.permissions().readonly()).unwrap_or(false);
        let fmt_time = |t: std::time::SystemTime| {
            chrono::DateTime::<chrono::Local>::from(t)
                .format("%d %b %Y, %H:%M")
                .to_string()
        };
        let modified_time = meta.as_ref().and_then(|m| m.modified().ok())
            .map(fmt_time).unwrap_or_else(|| "—".into());
        let created_time = meta.as_ref().and_then(|m| m.created().ok())
            .map(fmt_time).unwrap_or_else(|| "—".into());
        let children_count = Some(children.len());

        Ok(FsNode {
            name: p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| req.clone()),
            path: req,
            node_type: "directory".into(),
            size,
            size_human: format_size(size),
            modified_time,
            created_time,
            readonly,
            extension: None,
            children_count,
            children: Some(children),
        })
    })
    .await
    .map_err(|e| e.to_string())??;

    // Store in cache
    if let Ok(mut cache) = get_cache().lock() {
        cache.insert(cache_key, (Instant::now(), node.clone()));
    }

    Ok(node)
}

/// Deletes a file or directory at the given absolute path.
#[tauri::command]
fn delete_item(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    let res = if p.is_dir() {
        std::fs::remove_dir_all(p).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(p).map_err(|e| e.to_string())
    };

    if res.is_ok() {
        if let Ok(mut cache) = get_cache().lock() {
            cache.retain(|k, _| !k.starts_with(&path));
            if let Some(parent) = p.parent() {
                let parent_str = parent.to_string_lossy().to_string();
                cache.retain(|k, _| !k.starts_with(&parent_str));
            }
        }
    }

    res
}

/// Creates an empty file at the given absolute path.
#[tauri::command]
fn create_file(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if p.exists() {
        return Err(format!("Already exists: {}", path));
    }
    std::fs::File::create(p).map_err(|e| e.to_string())?;
    // Invalidate parent cache
    if let Some(parent) = p.parent() {
        let parent_str = parent.to_string_lossy().to_string();
        if let Ok(mut cache) = get_cache().lock() {
            cache.retain(|k, _| !k.starts_with(&parent_str));
        }
    }
    Ok(())
}

/// Creates a directory at the given absolute path.
#[tauri::command]
fn create_folder(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if p.exists() {
        return Err(format!("Already exists: {}", path));
    }
    std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    // Invalidate parent cache
    if let Some(parent) = p.parent() {
        let parent_str = parent.to_string_lossy().to_string();
        if let Ok(mut cache) = get_cache().lock() {
            cache.retain(|k, _| !k.starts_with(&parent_str));
        }
    }
    Ok(())
}

/// Moves a file or directory from `from` to `to`.
#[tauri::command]
fn move_item(from: String, to: String) -> Result<(), String> {
    let src = Path::new(&from);
    let dst = Path::new(&to);
    if !src.exists() {
        return Err(format!("Source not found: {}", from));
    }
    if dst.exists() {
        return Err(format!("Destination already exists: {}", to));
    }
    std::fs::rename(src, dst).map_err(|e| e.to_string())?;
    // Invalidate caches for source and destination parents
    if let Ok(mut cache) = get_cache().lock() {
        cache.retain(|k, _| !k.starts_with(&from));
        if let Some(parent) = src.parent() {
            let parent_str = parent.to_string_lossy().to_string();
            cache.retain(|k, _| !k.starts_with(&parent_str));
        }
        if let Some(parent) = dst.parent() {
            let parent_str = parent.to_string_lossy().to_string();
            cache.retain(|k, _| !k.starts_with(&parent_str));
        }
    }
    Ok(())
}

/// Opens a new Tracer window navigated to the given path.
#[tauri::command]
async fn open_in_new_window(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let label   = format!("tracer-{}", ts);
    let encoded = urlencoding::encode(&path);
    tauri::WebviewWindowBuilder::new(
        &app,
        label,
        tauri::WebviewUrl::App(format!("index.html?path={}", encoded).into()),
    )
    .title(format!("Tracer — {}", path.split('/').next_back().unwrap_or(&path)))
    .inner_size(1400.0, 900.0)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Returns the current user's home directory path.
#[tauri::command]
fn get_home_dir() -> String {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .to_string_lossy()
        .to_string()
}

// ─── App entry point ─────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let sessions: Arc<Mutex<HashMap<String, transfer::TransferSession>>> =
                Arc::new(Mutex::new(HashMap::new()));
            let peers: Arc<Mutex<HashMap<String, transfer::PeerInfo>>> =
                Arc::new(Mutex::new(HashMap::new()));

            let device_name = hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "Tracer".to_string());

            let app_handle = app.handle().clone();
            let sessions_srv = sessions.clone();
            let dev_name_srv = device_name.clone();

            let port = tauri::async_runtime::block_on(async move {
                transfer::server::start_server(sessions_srv, app_handle, dev_name_srv).await
            });

            transfer::discovery::start_discovery(
                peers.clone(),
                app.handle().clone(),
                &device_name,
                port,
            )
            .unwrap_or_else(|e| eprintln!("mDNS init failed: {e}"));

            app.manage(TransferAppState {
                sessions,
                peers,
                server_port: port,
                device_name,
            });

            let db_path = app.path().app_data_dir()
                .map(|d| d.join("graph.db"))
                .unwrap_or_else(|_| std::path::PathBuf::from("graph.db"));

            match graph::GraphAppState::new(&db_path) {
                Ok(graph_state) => { app.manage(graph_state); }
                Err(e) => eprintln!("[graph] failed to init: {e}"),
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_filesystem,
            delete_item,
            get_home_dir,
            create_file,
            create_folder,
            move_item,
            open_in_new_window,
            transfer::commands::get_peers,
            transfer::commands::start_transfer,
            transfer::commands::accept_transfer,
            transfer::commands::reject_transfer,
            transfer::commands::cancel_transfer,
            graph::graph_search,
            graph::graph_get_related,
            graph::graph_get_duplicates,
            graph::graph_index_status,
            graph::graph_set_root,
            graph::graph_set_llm,
            graph::graph_add_indexed_folder,
            graph::graph_remove_indexed_folder,
            graph::graph_list_indexed_folders,
            graph::graph_content_search,
            graph::graph_get_imports,
            graph::graph_get_importers,
            graph::graph_get_dep_tree,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tracer");
}
