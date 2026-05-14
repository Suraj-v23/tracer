# Knowledge Graph — Phase 4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add semantic vector search — embed file contents via ollama or OpenAI API, store vectors in SQLite, index with usearch HNSW for fast similarity queries, and expose "Find Similar Files" and semantic search commands.

**Architecture:** New `embedder.rs` generates embeddings from FTS content (Phase 2) using configurable providers. Vectors stored as BLOBs in a new `embeddings` SQLite table; a `usearch` HNSW index is held in-memory in `GraphAppState` for sub-50ms ANN queries. `graph_search` is upgraded to fan out to vector search and merge results. Embedding runs lazily — only after user sets an embedding provider.

**Tech Stack:** `usearch = "2"` (HNSW index, pure Rust), `bytemuck = "1"` (f32↔bytes), existing `reqwest` (embedding API calls), existing SQLite + FTS5.

---

## File Map

**Create (Rust):**
- `src-tauri/src/graph/embedder.rs` — embedding pipeline: call provider, chunk text, store vector, build HNSW

**Modify (Rust):**
- `src-tauri/Cargo.toml` — add `usearch`, `bytemuck`
- `src-tauri/src/graph/store.rs` — add `embeddings` table, `upsert_embedding`, `get_all_embeddings`, `get_node_path_by_id`
- `src-tauri/src/graph/mod.rs` — add `pub mod embedder`, add `hnsw_index` + `embed_config` to `GraphAppState`, add 3 commands, upgrade `graph_search`
- `src-tauri/src/graph/query.rs` — add `SemanticSearch` StructuredQuery variant

**Modify (Frontend):**
- `frontend/js/graph.ts` — add 3 API bindings + `EmbedConfig` type
- `frontend/js/graphui.ts` — add `showSimilar()` function
- `frontend/index.html` — add "Find Similar" context menu item
- `frontend/js/events.ts` — wire "Find Similar" menu item

---

## Task 1: Add usearch + bytemuck dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add dependencies**

Add to `[dependencies]` in `src-tauri/Cargo.toml`:

```toml
usearch = "2"
bytemuck = { version = "1", features = ["derive"] }
```

- [ ] **Step 2: Verify**

```bash
cd src-tauri && cargo fetch 2>&1 | tail -3
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/Cargo.toml
git -C /Users/suraj/Documents/tracer commit -m "chore(graph/p4): add usearch and bytemuck dependencies"
```

---

## Task 2: store.rs — embeddings table + CRUD

**Files:**
- Modify: `src-tauri/src/graph/store.rs`

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)]` block in `store.rs`:

```rust
#[test]
fn upsert_and_get_embedding() {
    let store = Store::open_in_memory().unwrap();
    store.upsert_node(&make_node("/a/f.ts", "f.ts", "file", 100)).unwrap();
    let id = store.get_node_id("/a/f.ts").unwrap().unwrap();
    let vec: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4];
    store.upsert_embedding(id, &vec).unwrap();

    let all = store.get_all_embeddings().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].0, id as u64);
    assert_eq!(all[0].1.len(), 4);
    assert!((all[0].1[0] - 0.1f32).abs() < 1e-6);
}

#[test]
fn get_node_path_by_id_works() {
    let store = Store::open_in_memory().unwrap();
    store.upsert_node(&make_node("/a/f.ts", "f.ts", "file", 100)).unwrap();
    let id = store.get_node_id("/a/f.ts").unwrap().unwrap();
    let path = store.get_node_path_by_id(id).unwrap();
    assert_eq!(path.as_deref(), Some("/a/f.ts"));
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cd src-tauri && cargo test graph::store::tests::upsert_and_get_embedding -- --nocapture 2>&1 | tail -5
```

Expected: compile error (methods don't exist yet).

- [ ] **Step 3: Add embeddings table to migrate()**

In `store.rs`, inside the `execute_batch` SQL string (after `fts_node_map`), add:

```sql
            CREATE TABLE IF NOT EXISTS embeddings (
                node_id INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
                vector  BLOB NOT NULL,
                dims    INTEGER NOT NULL
            );
```

- [ ] **Step 4: Add CRUD methods to Store impl**

Add after `importer_count`:

```rust
// ── Vector Embeddings ─────────────────────────────────────────────────────

pub fn upsert_embedding(&self, node_id: i64, vector: &[f32]) -> SqlResult<()> {
    let bytes: &[u8] = bytemuck::cast_slice(vector);
    self.conn.execute(
        "INSERT OR REPLACE INTO embeddings (node_id, vector, dims) VALUES (?1, ?2, ?3)",
        params![node_id, bytes, vector.len() as i64],
    )?;
    Ok(())
}

/// Returns (node_id_as_u64, vector) for all embedded nodes.
pub fn get_all_embeddings(&self) -> SqlResult<Vec<(u64, Vec<f32>)>> {
    let mut stmt = self.conn.prepare(
        "SELECT node_id, vector FROM embeddings"
    )?;
    let results = stmt.query_map([], |row| {
        let node_id: i64 = row.get(0)?;
        let bytes: Vec<u8> = row.get(1)?;
        Ok((node_id as u64, bytes))
    })?
    .filter_map(|r| r.ok())
    .map(|(id, bytes)| {
        let floats: Vec<f32> = bytemuck::cast_slice(&bytes).to_vec();
        (id, floats)
    })
    .collect();
    Ok(results)
}

pub fn get_node_path_by_id(&self, node_id: i64) -> SqlResult<Option<String>> {
    let mut stmt = self.conn.prepare_cached("SELECT path FROM nodes WHERE id=?1")?;
    let mut rows = stmt.query(params![node_id])?;
    Ok(rows.next()?.map(|r| r.get(0)).transpose()?)
}

pub fn get_nodes_by_ids(&self, ids: &[i64]) -> SqlResult<Vec<SearchResult>> {
    if ids.is_empty() { return Ok(vec![]); }
    let placeholders: String = ids.iter().enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT path, name, kind, size, extension, modified_secs FROM nodes WHERE id IN ({})",
        placeholders
    );
    let params_ref: Vec<&dyn rusqlite::ToSql> = ids.iter()
        .map(|id| id as &dyn rusqlite::ToSql)
        .collect();
    let mut stmt = self.conn.prepare(&sql)?;
    let results = stmt.query_map(params_ref.as_slice(), row_to_result)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(results)
}
```

Note: add `use bytemuck;` at the top of store.rs.

- [ ] **Step 5: Run tests**

```bash
cd src-tauri && cargo test graph::store -- --nocapture 2>&1 | tail -10
```

Expected: all 15 tests pass (13 original + 2 new).

- [ ] **Step 6: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/store.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p4): embeddings table, upsert_embedding, get_all_embeddings, get_nodes_by_ids"
```

---

## Task 3: embedder.rs — embedding pipeline + HNSW index

**Files:**
- Create: `src-tauri/src/graph/embedder.rs`

- [ ] **Step 1: Write failing tests first**

Create `src-tauri/src/graph/embedder.rs` starting with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec_to_bytes_roundtrip() {
        let v: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
        let bytes = vec_to_bytes(&v);
        let back = bytes_to_vec(&bytes);
        assert_eq!(back.len(), 4);
        assert!((back[0] - 1.0f32).abs() < 1e-6);
        assert!((back[3] - 4.0f32).abs() < 1e-6);
    }

    #[test]
    fn chunk_text_splits_long_text() {
        let text = "word ".repeat(600); // 600 words
        let chunks = chunk_text(&text, 512);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            let word_count = chunk.split_whitespace().count();
            assert!(word_count <= 512);
        }
    }

    #[test]
    fn chunk_text_short_text_single_chunk() {
        let text = "short text here";
        let chunks = chunk_text(text, 512);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn build_hnsw_and_search() {
        let index = build_hnsw_index(4);
        index.reserve(10).unwrap();
        index.add(1, &[1.0f32, 0.0, 0.0, 0.0]).unwrap();
        index.add(2, &[0.0f32, 1.0, 0.0, 0.0]).unwrap();
        index.add(3, &[1.0f32, 0.1, 0.0, 0.0]).unwrap(); // close to key=1

        let results = index.search(&[1.0f32, 0.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(results.keys[0], 1); // exact match first
        assert_eq!(results.keys[1], 3); // closest second
    }
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cd src-tauri && cargo test graph::embedder -- --nocapture 2>&1 | tail -5
```

Expected: compile error (module doesn't exist yet).

- [ ] **Step 3: Implement embedder.rs**

Replace the file with the full implementation:

```rust
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

// ─── Vector helpers ───────────────────────────────────────────────────────────

pub fn vec_to_bytes(v: &[f32]) -> Vec<u8> {
    bytemuck::cast_slice(v).to_vec()
}

pub fn bytes_to_vec(b: &[u8]) -> Vec<f32> {
    bytemuck::cast_slice(b).to_vec()
}

// ─── Text chunking ────────────────────────────────────────────────────────────

/// Split text into chunks of at most `max_words` words.
pub fn chunk_text(text: &str, max_words: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        return vec![text.to_string()];
    }
    words.chunks(max_words)
        .map(|chunk| chunk.join(" "))
        .collect()
}

// ─── HNSW index ───────────────────────────────────────────────────────────────

pub fn build_hnsw_index(dims: usize) -> Index {
    let options = IndexOptions {
        dimensions: dims,
        metric: MetricKind::Cos,
        quantization: ScalarKind::F32,
        connectivity: 16,
        expansion_add: 128,
        expansion_search: 64,
        ..Default::default()
    };
    Index::new(&options).expect("failed to create usearch index")
}

/// Load all stored embeddings from SQLite into a fresh HNSW index.
pub fn load_hnsw_from_store(store: &crate::graph::store::Store) -> Index {
    let all = store.get_all_embeddings().unwrap_or_default();
    if all.is_empty() {
        return build_hnsw_index(384); // default dims
    }
    let dims = all[0].1.len();
    let index = build_hnsw_index(dims);
    index.reserve(all.len()).ok();
    for (id, vec) in &all {
        index.add(*id, vec.as_slice()).ok();
    }
    index
}

// ─── Embedding API calls ──────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbedConfig {
    pub provider: String,    // "ollama" | "remote"
    pub base_url: String,    // "http://localhost:11434" for ollama
    pub model:    String,    // "nomic-embed-text" | "text-embedding-3-small"
    pub api_key:  Option<String>,
    pub dims:     usize,     // 384 for nomic-embed-text, 1536 for text-embedding-3-small
}

impl Default for EmbedConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            model:    "nomic-embed-text".into(),
            api_key:  None,
            dims:     384,
        }
    }
}

pub async fn embed_text(text: &str, config: &EmbedConfig) -> Result<Vec<f32>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    match config.provider.as_str() {
        "ollama" => {
            let url = format!("{}/api/embed", config.base_url);
            let body = serde_json::json!({ "model": config.model, "input": text });
            let resp = client.post(&url).json(&body).send().await
                .map_err(|e| format!("Ollama embed unreachable: {e}"))?
                .json::<serde_json::Value>().await
                .map_err(|e| format!("Ollama embed parse failed: {e}"))?;
            resp["embeddings"][0].as_array()
                .ok_or_else(|| "Ollama embed: missing embeddings[0]".to_string())?
                .iter()
                .map(|v| v.as_f64().map(|f| f as f32)
                    .ok_or_else(|| "non-float in embedding".to_string()))
                .collect()
        }
        "remote" => {
            let url = format!("{}/embeddings", config.base_url);
            let api_key = config.api_key.as_deref().unwrap_or("");
            let body = serde_json::json!({ "model": config.model, "input": text });
            let resp = client.post(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .json(&body).send().await
                .map_err(|e| format!("Remote embed unreachable: {e}"))?
                .json::<serde_json::Value>().await
                .map_err(|e| format!("Remote embed parse failed: {e}"))?;
            resp["data"][0]["embedding"].as_array()
                .ok_or_else(|| "Remote embed: missing data[0].embedding".to_string())?
                .iter()
                .map(|v| v.as_f64().map(|f| f as f32)
                    .ok_or_else(|| "non-float in embedding".to_string()))
                .collect()
        }
        other => Err(format!("Unknown embed provider: '{other}'. Use 'ollama' or 'remote'.")),
    }
}

// ─── Batch indexing ───────────────────────────────────────────────────────────

/// Embed all FTS-indexed content that doesn't have an embedding yet.
/// Returns count of newly embedded files.
pub async fn embed_all_content(
    store: &crate::graph::store::Store,
    config: &EmbedConfig,
    hnsw: &Index,
) -> usize {
    // Get node_ids that have FTS content but no embedding
    let candidates: Vec<(i64, String)> = {
        let mut stmt = match store.conn.prepare(r#"
            SELECT m.node_id, f.content
            FROM fts_node_map m
            JOIN fts_content f ON f.rowid = m.rowid
            LEFT JOIN embeddings e ON e.node_id = m.node_id
            WHERE e.node_id IS NULL
            LIMIT 500
        "#) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        match stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(_) => return 0,
        }
    };

    let mut count = 0;
    for (node_id, content) in candidates {
        // Use first chunk only for now (mean-pooling across chunks is Phase 5 concern)
        let chunks = chunk_text(&content, 512);
        let text = &chunks[0];
        match embed_text(text, config).await {
            Ok(vec) => {
                if store.upsert_embedding(node_id, &vec).is_ok() {
                    hnsw.reserve(hnsw.size() + 1).ok();
                    hnsw.add(node_id as u64, &vec).ok();
                    count += 1;
                }
            }
            Err(e) => eprintln!("[embedder] failed node {node_id}: {e}"),
        }
    }
    count
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec_to_bytes_roundtrip() {
        let v: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
        let bytes = vec_to_bytes(&v);
        let back = bytes_to_vec(&bytes);
        assert_eq!(back.len(), 4);
        assert!((back[0] - 1.0f32).abs() < 1e-6);
        assert!((back[3] - 4.0f32).abs() < 1e-6);
    }

    #[test]
    fn chunk_text_splits_long_text() {
        let text = "word ".repeat(600);
        let chunks = chunk_text(&text, 512);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            let word_count = chunk.split_whitespace().count();
            assert!(word_count <= 512);
        }
    }

    #[test]
    fn chunk_text_short_text_single_chunk() {
        let text = "short text here";
        let chunks = chunk_text(text, 512);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn build_hnsw_and_search() {
        let index = build_hnsw_index(4);
        index.reserve(10).unwrap();
        index.add(1, &[1.0f32, 0.0, 0.0, 0.0]).unwrap();
        index.add(2, &[0.0f32, 1.0, 0.0, 0.0]).unwrap();
        index.add(3, &[1.0f32, 0.1, 0.0, 0.0]).unwrap();

        let results = index.search(&[1.0f32, 0.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(results.keys[0], 1);
        assert_eq!(results.keys[1], 3);
    }
}
```

- [ ] **Step 4: Add `pub mod embedder;` to mod.rs**

Add after `pub mod parser;` in `src-tauri/src/graph/mod.rs`:
```rust
pub mod embedder;
```

Also add `use bytemuck;` at the top of `store.rs` if not already present (needed for the embeddings CRUD).

Actually, the `bytemuck` calls are in `store.rs` methods directly — add this import to the top of store.rs:
```rust
use bytemuck;
```

Wait — `bytemuck::cast_slice` is used directly in store.rs. Add to the top of store.rs:
```rust
// bytemuck is used for f32 ↔ bytes conversion in embedding methods
```

No import needed since it's used as `bytemuck::cast_slice` (full path). ✓

- [ ] **Step 5: Run tests**

```bash
cd src-tauri && cargo test graph::embedder -- --nocapture 2>&1 | tail -12
```

Expected: 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/embedder.rs src-tauri/src/graph/mod.rs src-tauri/src/graph/store.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p4): embedder.rs — HNSW index, embedding pipeline, text chunking"
```

---

## Task 4: GraphAppState + 3 Tauri commands + graph_search upgrade

**Files:**
- Modify: `src-tauri/src/graph/mod.rs`
- Modify: `src-tauri/src/graph/query.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add hnsw_index + embed_config to GraphAppState**

In `src-tauri/src/graph/mod.rs`, replace the `GraphAppState` struct with:

```rust
pub struct GraphAppState {
    pub store:        Arc<Mutex<Store>>,
    pub llm_config:   Arc<Mutex<Option<llm::LlmConfig>>>,
    pub indexed_root: Arc<Mutex<Option<String>>>,
    pub stats:        Arc<Mutex<indexer::IndexStats>>,
    pub embed_config: Arc<Mutex<Option<embedder::EmbedConfig>>>,
    pub hnsw:         Arc<Mutex<usearch::Index>>,
}
```

Replace `GraphAppState::new` with:

```rust
impl GraphAppState {
    pub fn new(db_path: &std::path::Path) -> Result<Self, String> {
        let store = Store::open(db_path).map_err(|e| e.to_string())?;
        // Load any existing embeddings into HNSW at startup
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
```

Also add at the top of mod.rs:
```rust
use usearch::Index as HnswIndex;
```

- [ ] **Step 2: Add 3 commands to mod.rs**

Add after `graph_get_dep_tree`:

```rust
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
    let config = state.embed_config.lock().map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "No embedding provider configured. Call graph_set_embedding_provider first.".to_string())?;

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

    // Get stored embedding for this file
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
        // Skip the first result if it's the file itself
        results.keys.iter()
            .filter(|&&id| id != node_id as u64)
            .take(top_k)
            .map(|k| *k as i64)
            .collect::<Vec<_>>()
    };

    let store = state.store.lock().map_err(|e| e.to_string())?;
    store.get_nodes_by_ids(&node_ids).map_err(|e| e.to_string())
}
```

Also add a `graph_embed_folder` command that triggers background embedding after deep-indexing:

```rust
#[tauri::command]
pub async fn graph_embed_folder(
    path: String,
    state: State<'_, GraphAppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let config = state.embed_config.lock().map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "No embedding provider configured.".to_string())?;

    let store_arc  = state.store.clone();
    let hnsw_arc   = state.hnsw.clone();

    tauri::async_runtime::spawn(async move {
        if let (Ok(store), Ok(hnsw)) = (store_arc.lock(), hnsw_arc.lock()) {
            let count = embedder::embed_all_content(&store, &config, &hnsw).await;
            eprintln!("[embedder] embedded {count} files in {path}");
            app.emit("graph-embeddings-ready", count).ok();
        }
    });

    Ok(())
}
```

- [ ] **Step 3: Upgrade graph_search to merge semantic results**

Replace the existing `graph_search` command with:

```rust
#[tauri::command]
pub async fn graph_search(
    query_str: String,
    state: State<'_, GraphAppState>,
) -> Result<Vec<SearchResult>, String> {
    let q = resolve_query(&query_str, &state).await;

    // Metadata + graph query
    let mut results = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        execute(&q, &store)?
    };

    // FTS fallthrough
    if results.is_empty() {
        if let Ok(store) = state.store.lock() {
            if let Ok(content_results) = store.content_search(&query_str) {
                results = content_results;
            }
        }
    }

    // Semantic fallthrough — only if embed provider configured and still empty
    if results.is_empty() {
        if let Ok(Some(config)) = state.embed_config.lock().map(|g| g.clone()) {
            if let Ok(query_vec) = embedder::embed_text(&query_str, &config).await {
                if let Ok(hnsw) = state.hnsw.lock() {
                    if hnsw.size() > 0 {
                        if let Ok(sem_results) = hnsw.search(&query_vec, 10) {
                            let ids: Vec<i64> = sem_results.keys.iter()
                                .map(|k| *k as i64).collect();
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
    }

    Ok(results)
}
```

- [ ] **Step 4: Add SemanticSearch to StructuredQuery in query.rs**

Add variant after `GetImporters`:
```rust
    SemanticSearch {
        query: String,
        #[serde(default = "default_k")] k: usize,
    },
```

Add `fn default_k() -> usize { 10 }` after `fn default_depth`.

Add match arm to `execute()` — SemanticSearch cannot be executed without the HNSW index, so return empty (caller handles via graph_semantic_search command):
```rust
        StructuredQuery::SemanticSearch { .. } => Ok(vec![]),
```

- [ ] **Step 5: Register commands in lib.rs**

Add after `graph::graph_get_dep_tree`:
```rust
graph::graph_set_embedding_provider,
graph::graph_semantic_search,
graph::graph_find_similar,
graph::graph_embed_folder,
```

- [ ] **Step 6: Build**

```bash
cd src-tauri && cargo build --lib 2>&1 | grep "^error" | head -10
```

Expected: no errors.

- [ ] **Step 7: Run all tests**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -5
```

Expected: all tests pass (53+).

- [ ] **Step 8: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/mod.rs src-tauri/src/graph/query.rs src-tauri/src/lib.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p4): semantic search commands, HNSW in AppState, graph_search merge"
```

---

## Task 5: Frontend — API bindings + "Find Similar" + settings

**Files:**
- Modify: `frontend/js/graph.ts`
- Modify: `frontend/js/graphui.ts`
- Modify: `frontend/index.html`
- Modify: `frontend/js/events.ts`

- [ ] **Step 1: Add to graph.ts** (after `graphGetDepTree`):

```typescript
export interface EmbedConfig {
    provider: 'ollama' | 'remote';
    base_url: string;
    model:    string;
    api_key?: string;
    dims:     number;
}

export async function graphSetEmbeddingProvider(config: EmbedConfig): Promise<void> {
    return _invoke('graph_set_embedding_provider', { config }) as Promise<void>;
}

export async function graphSemanticSearch(query: string, k?: number): Promise<GraphSearchResult[]> {
    return _invoke('graph_semantic_search', { query, k }) as Promise<GraphSearchResult[]>;
}

export async function graphFindSimilar(path: string, k?: number): Promise<GraphSearchResult[]> {
    return _invoke('graph_find_similar', { path, k }) as Promise<GraphSearchResult[]>;
}

export async function graphEmbedFolder(path: string): Promise<void> {
    return _invoke('graph_embed_folder', { path }) as Promise<void>;
}
```

- [ ] **Step 2: Add showSimilar + embeddings ready listener to graphui.ts**

Add after `showImports`:

```typescript
export async function showSimilar(path: string): Promise<void> {
    const name = path.split('/').pop() || path;
    const panel = document.getElementById('graph-results-panel')!;
    panel.classList.remove('hidden');
    panel.innerHTML = '<div class="graph-results-loading">Finding similar files…</div>';

    try {
        const results = await graphApi.graphFindSimilar(path, 10);
        if (!results.length) {
            panel.innerHTML = `<div class="graph-results-empty">No similar files found for ${_escHtml(name)}.<br><small>Deep-index + embed the folder first.</small></div>`;
            return;
        }
        const items = results.map(r => `
            <div class="graph-result-item" data-path="${_escHtml(r.path)}" title="${_escHtml(r.path)}">
                <span class="gr-icon">${r.kind === 'directory' ? '📁' : '📄'}</span>
                <span class="gr-name">${_escHtml(r.name)}</span>
                <span class="gr-size">${r.size_human}</span>
            </div>
        `).join('');
        panel.innerHTML = `
            <div class="graph-results-header">
                <span>Files similar to ${_escHtml(name)}</span>
                <button id="graph-results-close" class="graph-results-close">✕</button>
            </div>
            <div class="graph-results-list">${items}</div>
        `;
        document.getElementById('graph-results-close')?.addEventListener('click', hideResultsPanel);
    } catch (e) {
        const msg = String(e);
        if (msg.includes('No embedding provider')) {
            panel.innerHTML = `<div class="graph-results-empty">Set an embedding provider first.<br><small>${_escHtml(msg)}</small></div>`;
        } else {
            panel.innerHTML = `<div class="graph-results-empty">Error: ${_escHtml(msg)}</div>`;
        }
    }
}
```

Also add to `initGraphUI`, after the `graph-content-indexed` listener:

```typescript
    if (tauri?.event?.listen) {
        tauri.event.listen('graph-embeddings-ready', (count: number) => {
            toast(`Semantic index ready — ${count} files embedded`, 'success');
        });
    }
```

- [ ] **Step 3: Add context menu item to index.html**

After `<div class="ctx-item" id="ctx-show-importers">`, add:

```html
<div class="ctx-item" id="ctx-find-similar">≈ Find Similar Files</div>
```

- [ ] **Step 4: Wire in events.ts**

Update graphui import to include `showSimilar`:
```typescript
import { addIndexedFolder, showImports, showSimilar } from './graphui.js';
```

Add handler in `bindGlobalEvents()` after the `ctx-show-importers` handler:
```typescript
    document.getElementById('ctx-find-similar')?.addEventListener('click', async () => {
        document.getElementById('ctx-menu')!.classList.add('hidden');
        if (state.ctxTarget) await showSimilar(state.ctxTarget.path);
    });
```

In `bindNodeContextMenu`, add visibility control for `ctx-find-similar` (show only for files, not directories):
```typescript
    const similarEl = document.getElementById('ctx-find-similar');
    if (similarEl) similarEl.style.display = item.type === 'file' ? '' : 'none';
```

- [ ] **Step 5: Compile**

```bash
npm run build:ts 2>&1
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git -C /Users/suraj/Documents/tracer add frontend/js/graph.ts frontend/js/graph.js frontend/js/graphui.ts frontend/js/graphui.js frontend/index.html frontend/js/events.ts frontend/js/events.js
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p4): frontend semantic search API, Find Similar context menu"
```

---

## Task 6: Final Verification

- [ ] **Step 1: Full test suite**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -8
```

Expected: all tests pass (53+ total).

- [ ] **Step 2: Clippy**

```bash
cd src-tauri && cargo clippy --lib 2>&1 | grep "^error" | head -5
```

Expected: no output.

- [ ] **Step 3: TypeScript**

```bash
npm run build:ts 2>&1
```

Expected: no errors.

- [ ] **Step 4: Smoke test**

```bash
npm run tauri dev
```

Test sequence (requires ollama running with `nomic-embed-text`):
1. Open app, right-click a code folder → "Deep Index This Folder"
2. Wait for "Content indexing complete" toast
3. Via dev console: `window.__TAURI_INTERNALS__.invoke('graph_set_embedding_provider', { config: { provider: 'ollama', base_url: 'http://localhost:11434', model: 'nomic-embed-text', dims: 384 } })`
4. Via dev console: `window.__TAURI_INTERNALS__.invoke('graph_embed_folder', { path: '/your/folder' })`
5. Wait for "Semantic index ready" toast
6. Right-click any `.ts` file → "≈ Find Similar Files" → results panel shows semantically similar files
7. Search tab → type a natural language query → results appear

Without ollama: steps 1-2 still work, steps 3-7 return "No embedding provider" error message.

- [ ] **Step 5: Final commit**

```bash
git -C /Users/suraj/Documents/tracer add -A
git -C /Users/suraj/Documents/tracer commit -m "feat: knowledge graph Phase 4 complete — semantic vector search" --allow-empty
```
