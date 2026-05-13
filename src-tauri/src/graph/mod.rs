pub mod store;
pub mod query;
pub mod indexer;
pub mod llm;

use std::path::Path;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use store::{SearchResult, Store};
use query::{execute, heuristic_parse, StructuredQuery};
use llm::LlmConfig;

// ─── App state ───────────────────────────────────────────────────────────────

pub struct GraphAppState {
    pub store:        Arc<Mutex<Store>>,
    pub llm_config:   Arc<Mutex<Option<LlmConfig>>>,
    pub indexed_root: Arc<Mutex<Option<String>>>,
    pub stats:        Arc<Mutex<indexer::IndexStats>>,
}

impl GraphAppState {
    pub fn new(db_path: &Path) -> Result<Self, String> {
        let store = Store::open(db_path).map_err(|e| e.to_string())?;
        Ok(Self {
            store:        Arc::new(Mutex::new(store)),
            llm_config:   Arc::new(Mutex::new(None)),
            indexed_root: Arc::new(Mutex::new(None)),
            stats:        Arc::new(Mutex::new(indexer::IndexStats::default())),
        })
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn resolve_query(query_str: &str, state: &GraphAppState) -> StructuredQuery {
    // Clone the config out before awaiting so the MutexGuard is not held across an await point.
    let maybe_config = state.llm_config.lock().unwrap().clone();
    if let Some(config) = maybe_config {
        if let Ok(q) = llm::nl_to_query(query_str, &config).await {
            return q;
        }
    }
    heuristic_parse(query_str)
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn graph_search(
    query_str: String,
    state: State<'_, GraphAppState>,
) -> Result<Vec<SearchResult>, String> {
    let q = resolve_query(&query_str, &state).await;
    let store = state.store.lock().map_err(|e| e.to_string())?;
    execute(&q, &store)
}

#[tauri::command]
pub async fn graph_get_related(
    path: String,
    state: State<'_, GraphAppState>,
) -> Result<Vec<SearchResult>, String> {
    let q = StructuredQuery::GetRelated { path, depth: 1 };
    let store = state.store.lock().map_err(|e| e.to_string())?;
    execute(&q, &store)
}

#[tauri::command]
pub async fn graph_get_duplicates(
    path: String,
    state: State<'_, GraphAppState>,
) -> Result<Vec<SearchResult>, String> {
    let q = StructuredQuery::FindDuplicates { path };
    let store = state.store.lock().map_err(|e| e.to_string())?;
    execute(&q, &store)
}

#[tauri::command]
pub async fn graph_index_status(
    state: State<'_, GraphAppState>,
) -> Result<indexer::IndexStats, String> {
    Ok(state.stats.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
pub async fn graph_set_root(
    path: String,
    state: State<'_, GraphAppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    *state.indexed_root.lock().map_err(|e| e.to_string())? = Some(path.clone());

    let store_arc = state.store.clone();
    let stats_arc = state.stats.clone();

    std::thread::spawn(move || {
        // Phase 1: collect file metadata WITHOUT holding the store lock.
        // This is the slow part (disk I/O) — searches can proceed freely.
        let (entries, nodes) = indexer::collect_nodes(Path::new(&path));
        let total = nodes.len();
        if let Ok(mut stats) = stats_arc.lock() { stats.total = total; }

        // Phase 2: insert in 500-node batches, releasing the lock between
        // batches so concurrent graph_search calls are never blocked for long.
        let mut indexed = 0usize;
        for chunk_nodes in nodes.chunks(500) {
            // Find the matching entries slice for parent-edge insertion.
            // We reuse insert_nodes on sub-slices; duplicate edges are added
            // at the end over the full node set to avoid missed cross-chunk pairs.
            if let Ok(store) = store_arc.lock() {
                for node in chunk_nodes {
                    if store.upsert_node(node).is_ok() { indexed += 1; }
                }
            } // lock released here — searches can proceed
            if let Ok(mut stats) = stats_arc.lock() { stats.indexed = indexed; }
        }

        // Insert parent + duplicate edges in one final brief lock.
        if let Ok(store) = store_arc.lock() {
            for entry in &entries {
                if let Some(parent) = entry.path().parent() {
                    let _ = store.upsert_edge(
                        &entry.path().to_string_lossy(),
                        &parent.to_string_lossy(),
                        "parent",
                    );
                }
            }
            indexer::insert_duplicate_edges_pub(&store, &nodes);
        }

        if let Ok(mut stats) = stats_arc.lock() {
            stats.indexed  = indexed;
            stats.watching = true;
        }
        app.emit("graph-index-complete", ()).ok();
        indexer::start_watcher(path, store_arc, app);
    });

    Ok(())
}

#[tauri::command]
pub async fn graph_set_llm(
    config: LlmConfig,
    state: State<'_, GraphAppState>,
) -> Result<(), String> {
    *state.llm_config.lock().map_err(|e| e.to_string())? = Some(config);
    Ok(())
}
