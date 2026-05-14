pub mod store;
pub mod query;
pub mod indexer;
pub mod llm;
pub mod content;
pub mod parser;
pub mod embedder;
pub mod community;

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
    pub llm_config:   Arc<Mutex<Option<llm::LlmConfig>>>,
    pub indexed_root: Arc<Mutex<Option<String>>>,
    pub stats:        Arc<Mutex<indexer::IndexStats>>,
    pub embed_config: Arc<Mutex<Option<embedder::EmbedConfig>>>,
    pub hnsw:         Arc<Mutex<usearch::Index>>,
}

impl GraphAppState {
    pub fn new(db_path: &Path) -> Result<Self, String> {
        let store = Store::open(db_path).map_err(|e| e.to_string())?;
        let hnsw = embedder::load_hnsw_from_store(&store);
        Ok(Self {
            store:        Arc::new(Mutex::new(store)),
            llm_config:   Arc::new(Mutex::new(None)),
            indexed_root: Arc::new(Mutex::new(None)),
            stats:        Arc::new(Mutex::new(indexer::IndexStats::default())),
            embed_config: Arc::new(Mutex::new(None)),
            hnsw:         Arc::new(Mutex::new(hnsw)),
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

    let mut results = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        execute(&q, &store)?
    };

    if results.is_empty() {
        if let Ok(store) = state.store.lock() {
            if let Ok(content_results) = store.content_search(&query_str) {
                results = content_results;
            }
        }
    }

    if results.is_empty() {
        // Clone config out before any await — drops MutexGuard immediately.
        let maybe_config: Option<embedder::EmbedConfig> =
            state.embed_config.lock().ok().and_then(|g| g.clone());
        if let Some(config) = maybe_config {
            if let Ok(query_vec) = embedder::embed_text(&query_str, &config).await {
                let hnsw_size = state.hnsw.lock().map(|h| h.size()).unwrap_or(0);
                if hnsw_size > 0 {
                    let sem_ids: Option<Vec<i64>> = state.hnsw.lock().ok().and_then(|h| {
                        h.search(&query_vec, 10).ok()
                            .map(|r| r.keys.iter().map(|k| *k as i64).collect())
                    });
                    if let Some(ids) = sem_ids {
                        if let Ok(store) = state.store.lock() {
                            if let Ok(sem_nodes) = store.get_nodes_by_ids(&ids) {
                                results = sem_nodes;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(results)
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
    let store_arc = state.store.clone();
    // Compute hashes + edges on first request (runs in background, CPU-bound)
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(store) = store_arc.lock() {
            indexer::compute_duplicates(&store);
        }
    }).await.map_err(|e| e.to_string())?;

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
        // Phase 1: collect from disk — no lock needed, slow I/O.
        let (entries, nodes) = indexer::collect_nodes(Path::new(&path));
        let total = nodes.len();
        if let Ok(mut s) = stats_arc.lock() { s.total = total; }

        // Phase 2: node inserts in 1000-node batches wrapped in SQLite
        // transactions (~100× faster than auto-commit). Lock released between
        // batches so graph_search is never blocked for more than ~50ms.
        let mut indexed = 0usize;
        for chunk in nodes.chunks(1000) {
            if let Ok(store) = store_arc.lock() {
                let _ = store.conn.execute("BEGIN", []);
                for node in chunk {
                    if store.upsert_node(node).is_ok() { indexed += 1; }
                }
                let _ = store.conn.execute("COMMIT", []);
            }
            if let Ok(mut s) = stats_arc.lock() { s.indexed = indexed; }
        }

        // Phase 3: parent edges in 2000-entry batches, each its own transaction.
        for chunk in entries.chunks(2000) {
            if let Ok(store) = store_arc.lock() {
                let _ = store.conn.execute("BEGIN", []);
                for entry in chunk {
                    if let Some(parent) = entry.path().parent() {
                        let _ = store.upsert_edge(
                            &entry.path().to_string_lossy(),
                            &parent.to_string_lossy(),
                            "parent",
                        );
                    }
                }
                let _ = store.conn.execute("COMMIT", []);
            }
        }

        // Duplicate detection deferred to first user request — skipped at scan time
        // to avoid saturating CPU with blake3 hashing across the whole filesystem.

        if let Ok(mut s) = stats_arc.lock() { s.indexed = indexed; s.watching = true; }
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

#[tauri::command]
pub async fn graph_add_indexed_folder(
    path: String,
    state: State<'_, GraphAppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store.add_indexed_folder(&path).map_err(|e| e.to_string())?;
    }

    let store_arc = state.store.clone();
    std::thread::spawn(move || {
        // Ensure nodes exist in graph (may already be from phase 1 scan)
        let (_, nodes) = crate::graph::indexer::collect_nodes(std::path::Path::new(&path));
        if let Ok(store) = store_arc.lock() {
            let _ = store.conn.execute("BEGIN", []);
            for node in &nodes { let _ = store.upsert_node(node); }
            let _ = store.conn.execute("COMMIT", []);
        }
        // Extract and index content (lock released between phases)
        if let Ok(store) = store_arc.lock() {
            crate::graph::content::index_folder(std::path::Path::new(&path), &store);
        }
        // Parse and store import edges
        if let Ok(store) = store_arc.lock() {
            crate::graph::parser::index_imports(std::path::Path::new(&path), &store);
        }
        app.emit("graph-content-indexed", &path).ok();
    });

    Ok(())
}

#[tauri::command]
pub async fn graph_remove_indexed_folder(
    path: String,
    state: State<'_, GraphAppState>,
) -> Result<(), String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.remove_indexed_folder(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn graph_list_indexed_folders(
    state: State<'_, GraphAppState>,
) -> Result<Vec<String>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.list_indexed_folders().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn graph_content_search(
    query: String,
    state: State<'_, GraphAppState>,
) -> Result<Vec<SearchResult>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.content_search(&query).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DepTree {
    pub path:    String,
    pub name:    String,
    pub imports: Vec<DepTree>,
}

#[tauri::command]
pub async fn graph_get_imports(
    path: String,
    state: State<'_, GraphAppState>,
) -> Result<Vec<SearchResult>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.get_imports(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn graph_get_importers(
    path: String,
    state: State<'_, GraphAppState>,
) -> Result<Vec<SearchResult>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.get_importers(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn graph_get_dep_tree(
    path: String,
    depth: Option<usize>,
    state: State<'_, GraphAppState>,
) -> Result<DepTree, String> {
    let max_depth = depth.unwrap_or(3);
    let store = state.store.lock().map_err(|e| e.to_string())?;
    build_dep_tree(&path, max_depth, 0, &store)
}

#[tauri::command]
pub async fn graph_set_embedding_provider(
    config: embedder::EmbedConfig,
    state: State<'_, GraphAppState>,
) -> Result<(), String> {
    *state.embed_config.lock().map_err(|e| e.to_string())? = Some(config);
    Ok(())
}

#[tauri::command]
pub async fn graph_semantic_search(
    query: String,
    k: Option<usize>,
    state: State<'_, GraphAppState>,
) -> Result<Vec<SearchResult>, String> {
    let top_k = k.unwrap_or(10);
    // Clone config out before any await — guard must not cross the await point.
    let config: embedder::EmbedConfig = {
        let guard = state.embed_config.lock().map_err(|e| e.to_string())?;
        guard.clone()
            .ok_or_else(|| "No embedding provider configured. Call graph_set_embedding_provider first.".to_string())?
    }; // guard dropped here

    let query_vec = embedder::embed_text(&query, &config).await?;

    let node_ids = {
        let hnsw = state.hnsw.lock().map_err(|e| e.to_string())?;
        if hnsw.size() == 0 {
            return Err("No embeddings indexed yet. Deep-index a folder first.".to_string());
        }
        let results = hnsw.search(&query_vec, top_k).map_err(|e| e.to_string())?;
        results.keys.iter().map(|k| *k as i64).collect::<Vec<_>>()
    };

    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.get_nodes_by_ids(&node_ids).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn graph_find_similar(
    path: String,
    k: Option<usize>,
    state: State<'_, GraphAppState>,
) -> Result<Vec<SearchResult>, String> {
    let top_k = k.unwrap_or(10);

    let node_id = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store.get_node_id(&path)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("File not indexed: {path}"))?
    };

    let stored_vec = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let all = store.get_all_embeddings().map_err(|e| e.to_string())?;
        all.into_iter()
            .find(|(id, _)| *id == node_id as u64)
            .map(|(_, v)| v)
            .ok_or_else(|| format!("No embedding for {path}. Deep-index the folder first."))?
    };

    let node_ids = {
        let hnsw = state.hnsw.lock().map_err(|e| e.to_string())?;
        let results = hnsw.search(&stored_vec, top_k + 1).map_err(|e| e.to_string())?;
        results.keys.iter()
            .filter(|&&id| id != node_id as u64)
            .take(top_k)
            .map(|k| *k as i64)
            .collect::<Vec<_>>()
    };

    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.get_nodes_by_ids(&node_ids).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn graph_embed_folder(
    path: String,
    state: State<'_, GraphAppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let config = state.embed_config.lock().map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "No embedding provider configured.".to_string())?;

    let store_arc = state.store.clone();
    let hnsw_arc  = state.hnsw.clone();

    tauri::async_runtime::spawn(async move {
        let count = embedder::embed_all_content(&store_arc, &config, &hnsw_arc).await;
        eprintln!("[embedder] embedded {count} files in {path}");
        app.emit("graph-embeddings-ready", count).ok();
    });

    Ok(())
}

// ─── GraphRAG response types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Community {
    pub id:      i64,
    pub label:   Option<String>,
    pub summary: Option<String>,
    pub size:    usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityDetail {
    pub id:      i64,
    pub label:   Option<String>,
    pub summary: Option<String>,
    pub members: Vec<SearchResult>,
}

// ─── GraphRAG commands ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn graph_rebuild_communities(
    state: State<'_, GraphAppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let store_arc  = state.store.clone();
    let llm_config = state.llm_config.lock().map_err(|e| e.to_string())?.clone();

    tauri::async_runtime::spawn(async move {
        if let Ok(store) = store_arc.lock() {
            community::rebuild_communities(&store);
        } // guard dropped before any await

        if let Some(config) = llm_config {
            community::summarize_communities(&store_arc, &config).await;
        }
        app.emit("graph-communities-ready", ()).ok();
    });

    Ok(())
}

#[tauri::command]
pub async fn graph_list_communities(
    state: State<'_, GraphAppState>,
) -> Result<Vec<Community>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.list_communities()
        .map_err(|e| e.to_string())
        .map(|cs| cs.into_iter().map(|c| {
            let size = serde_json::from_str::<Vec<i64>>(&c.member_ids)
                .map(|v| v.len()).unwrap_or(0);
            Community { id: c.id, label: c.label, summary: c.summary, size }
        }).collect())
}

#[tauri::command]
pub async fn graph_get_community(
    id: i64,
    state: State<'_, GraphAppState>,
) -> Result<CommunityDetail, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let communities = store.list_communities().map_err(|e| e.to_string())?;
    let c = communities.into_iter().find(|c| c.id == id)
        .ok_or_else(|| format!("Community {id} not found"))?;
    let members = store.get_community_members(id).map_err(|e| e.to_string())?;
    Ok(CommunityDetail { id: c.id, label: c.label, summary: c.summary, members })
}

#[tauri::command]
pub async fn graph_global_query(
    question: String,
    state: State<'_, GraphAppState>,
) -> Result<community::GlobalAnswer, String> {
    let config = state.llm_config.lock().map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "No LLM configured. Set one via graph_set_llm.".to_string())?;
    let store_arc = state.store.clone();
    community::global_query(&question, &store_arc, &config).await
}

fn build_dep_tree(path: &str, max_depth: usize, current: usize, store: &Store)
    -> Result<DepTree, String>
{
    let name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    let imports = if current < max_depth {
        store.get_imports(path)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter_map(|r| build_dep_tree(&r.path, max_depth, current + 1, store).ok())
            .collect()
    } else {
        vec![]
    };

    Ok(DepTree { path: path.to_string(), name, imports })
}
