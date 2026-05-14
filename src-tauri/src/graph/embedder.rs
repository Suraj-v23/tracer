use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

// ─── Vector helpers ───────────────────────────────────────────────────────────

pub fn vec_to_bytes(v: &[f32]) -> Vec<u8> {
    bytemuck::cast_slice(v).to_vec()
}

pub fn bytes_to_vec(b: &[u8]) -> Vec<f32> {
    bytemuck::cast_slice(b).to_vec()
}

// ─── Text chunking ────────────────────────────────────────────────────────────

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

pub fn load_hnsw_from_store(store: &crate::graph::store::Store) -> Index {
    let all = store.get_all_embeddings().unwrap_or_default();
    if all.is_empty() {
        return build_hnsw_index(384);
    }
    let dims = all[0].1.len();
    let index = build_hnsw_index(dims);
    index.reserve(all.len()).ok();
    for (id, vec) in &all {
        index.add(*id, vec.as_slice()).ok();
    }
    index
}

// ─── Embedding config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbedConfig {
    pub provider: String,
    pub base_url: String,
    pub model:    String,
    pub api_key:  Option<String>,
    pub dims:     usize,
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

// ─── Embedding API calls ──────────────────────────────────────────────────────

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

pub async fn embed_all_content(
    store: &std::sync::Arc<std::sync::Mutex<crate::graph::store::Store>>,
    config: &EmbedConfig,
    hnsw:  &std::sync::Arc<std::sync::Mutex<Index>>,
) -> usize {
    // Collect candidates with the lock held only for the sync DB read, then drop it.
    let candidates: Vec<(i64, String)> = {
        let Ok(s) = store.lock() else { return 0 };
        let sql = r#"
            SELECT m.node_id, f.content
            FROM fts_node_map m
            JOIN fts_content f ON f.rowid = m.rowid
            LEFT JOIN embeddings e ON e.node_id = m.node_id
            WHERE e.node_id IS NULL
            LIMIT 500
        "#;
        let mut stmt = match s.conn.prepare(sql) {
            Ok(st) => st,
            Err(_)  => return 0,
        };
        let rows = match stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) {
            Ok(r)  => r,
            Err(_) => return 0,
        };
        rows.filter_map(|r| r.ok()).collect()
    }; // lock released here — no guard crosses the await below

    let mut count = 0;
    for (node_id, content) in candidates {
        let text = chunk_text(&content, 512).into_iter().next().unwrap_or_default();
        // HTTP call — no lock held
        match embed_text(&text, config).await {
            Ok(vec) => {
                // Re-acquire store lock just for the upsert
                let store_ok = store.lock()
                    .map(|s| s.upsert_embedding(node_id, &vec).is_ok())
                    .unwrap_or(false);
                if store_ok {
                    // Re-acquire hnsw lock just for the insert
                    if let Ok(h) = hnsw.lock() {
                        h.reserve(h.size() + 1).ok();
                        h.add(node_id as u64, &vec).ok();
                    }
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
