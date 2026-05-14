# Knowledge Graph — Phase 5 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add GraphRAG-style community intelligence — extract code entities (functions/classes/symbols), detect communities of related files via label propagation, generate LLM summaries per community, and answer global questions ("what does this codebase do?") using two-level community-guided retrieval.

**Architecture:** New `community.rs` handles entity extraction (regex), label propagation community detection, LLM summarization, and two-level GraphRAG query. `store.rs` gains `entities` + `communities` tables. `mod.rs` adds 4 commands. The "Ask AI" search mode now calls `graph_global_query` and shows an answer panel with LLM response + source citations. A "Communities" sidebar section lists detected clusters.

**Tech Stack:** `regex` (existing), `serde_json` (existing), `llm.rs` (existing `LlmConfig` + HTTP call), `petgraph` (existing — for adjacency traversal), TypeScript (existing pipeline).

---

## File Map

**Create (Rust):**
- `src-tauri/src/graph/community.rs` — entity extraction, label propagation, summarization, global query

**Modify (Rust):**
- `src-tauri/src/graph/store.rs` — add entities + communities tables, CRUD methods
- `src-tauri/src/graph/llm.rs` — expose `call_llm_raw` (make inner fn pub)
- `src-tauri/src/graph/mod.rs` — add `pub mod community`, 4 new commands, add `GlobalAnswer`/`Community`/`CommunityDetail` types
- `src-tauri/src/lib.rs` — register 4 new commands

**Modify (Frontend):**
- `frontend/js/graph.ts` — add 4 API bindings + `GlobalAnswer`/`Community` types
- `frontend/js/graphui.ts` — wire Ask AI mode to `graph_global_query`, answer panel, communities list
- `frontend/index.html` — add answer panel HTML, communities section in results
- `frontend/css/style.css` — answer panel styles

---

## Task 1: store.rs — entities + communities tables + CRUD

**Files:**
- Modify: `src-tauri/src/graph/store.rs`

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)]` block in `store.rs`:

```rust
#[test]
fn entity_insert_and_list() {
    let store = Store::open_in_memory().unwrap();
    store.upsert_node(&make_node("/a/main.ts", "main.ts", "file", 100)).unwrap();
    let id = store.get_node_id("/a/main.ts").unwrap().unwrap();

    store.insert_entities(id, &[
        ("authenticate".to_string(), "function".to_string()),
        ("User".to_string(), "class".to_string()),
    ]).unwrap();

    let names = store.get_entity_names_for_node(id).unwrap();
    assert!(names.contains(&"authenticate".to_string()));
    assert!(names.contains(&"User".to_string()));
}

#[test]
fn community_crud() {
    let store = Store::open_in_memory().unwrap();
    let id = store.upsert_community(None, None, &[1, 2, 3]).unwrap();
    assert!(id > 0);

    store.update_community_summary(id, "Auth module").unwrap();

    let communities = store.list_communities().unwrap();
    assert_eq!(communities.len(), 1);
    assert_eq!(communities[0].summary.as_deref(), Some("Auth module"));
    assert_eq!(communities[0].id, id);

    let detail = store.get_community_members(id).unwrap();
    assert_eq!(detail.len(), 3);
}

#[test]
fn get_all_node_entities_groups_by_node() {
    let store = Store::open_in_memory().unwrap();
    store.upsert_node(&make_node("/a.ts", "a.ts", "file", 10)).unwrap();
    store.upsert_node(&make_node("/b.ts", "b.ts", "file", 10)).unwrap();
    let id_a = store.get_node_id("/a.ts").unwrap().unwrap();
    let id_b = store.get_node_id("/b.ts").unwrap().unwrap();

    store.insert_entities(id_a, &[("foo".to_string(), "function".to_string())]).unwrap();
    store.insert_entities(id_b, &[("foo".to_string(), "function".to_string()),
                                   ("bar".to_string(), "function".to_string())]).unwrap();

    let all = store.get_all_node_entities().unwrap();
    assert_eq!(all.len(), 2);
    let a_entities = all.iter().find(|(id, _)| *id == id_a).map(|(_, e)| e).unwrap();
    assert_eq!(a_entities.len(), 1);
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cd src-tauri && cargo test graph::store::tests::entity_insert_and_list -- --nocapture 2>&1 | tail -5
```

Expected: compile error.

- [ ] **Step 3: Add tables to migrate() SQL**

After `embeddings` table in the `execute_batch` string, add:

```sql
            CREATE TABLE IF NOT EXISTS entities (
                id      INTEGER PRIMARY KEY,
                node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                name    TEXT NOT NULL,
                kind    TEXT NOT NULL DEFAULT 'symbol'
            );
            CREATE INDEX IF NOT EXISTS idx_entities_node ON entities(node_id);
            CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(name);

            CREATE TABLE IF NOT EXISTS communities (
                id         INTEGER PRIMARY KEY,
                label      TEXT,
                summary    TEXT,
                member_ids TEXT NOT NULL DEFAULT '[]'
            );
```

- [ ] **Step 4: Add entity + community methods to Store impl**

Add after `get_nodes_by_ids`:

```rust
// ── Entities ──────────────────────────────────────────────────────────────

pub fn insert_entities(&self, node_id: i64, entities: &[(String, String)]) -> SqlResult<()> {
    // Clear old entities for this node first
    self.conn.execute("DELETE FROM entities WHERE node_id=?1", params![node_id])?;
    for (name, kind) in entities {
        self.conn.execute(
            "INSERT INTO entities (node_id, name, kind) VALUES (?1, ?2, ?3)",
            params![node_id, name, kind],
        )?;
    }
    Ok(())
}

pub fn get_entity_names_for_node(&self, node_id: i64) -> SqlResult<Vec<String>> {
    let mut stmt = self.conn.prepare(
        "SELECT name FROM entities WHERE node_id=?1"
    )?;
    let results = stmt.query_map(params![node_id], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(results)
}

/// Returns (node_id, Vec<entity_name>) for all nodes with entities.
pub fn get_all_node_entities(&self) -> SqlResult<Vec<(i64, Vec<String>)>> {
    let mut stmt = self.conn.prepare(
        "SELECT node_id, name FROM entities ORDER BY node_id"
    )?;
    let rows: Vec<(i64, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut grouped: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    for (id, name) in rows {
        grouped.entry(id).or_default().push(name);
    }
    Ok(grouped.into_iter().collect())
}

// ── Communities ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommunitySummary {
    pub id:         i64,
    pub label:      Option<String>,
    pub summary:    Option<String>,
    pub member_ids: String,   // JSON array
}

pub fn upsert_community(
    &self,
    label: Option<&str>,
    summary: Option<&str>,
    member_node_ids: &[i64],
) -> SqlResult<i64> {
    let member_json = serde_json::to_string(member_node_ids).unwrap_or_else(|_| "[]".into());
    self.conn.execute(
        "INSERT INTO communities (label, summary, member_ids) VALUES (?1, ?2, ?3)",
        params![label, summary, member_json],
    )?;
    Ok(self.conn.last_insert_rowid())
}

pub fn update_community_summary(&self, id: i64, summary: &str) -> SqlResult<()> {
    self.conn.execute(
        "UPDATE communities SET summary=?1 WHERE id=?2",
        params![summary, id],
    )?;
    Ok(())
}

pub fn list_communities(&self) -> SqlResult<Vec<CommunitySummary>> {
    let mut stmt = self.conn.prepare(
        "SELECT id, label, summary, member_ids FROM communities ORDER BY id"
    )?;
    let results = stmt.query_map([], |r| Ok(CommunitySummary {
        id:         r.get(0)?,
        label:      r.get(1)?,
        summary:    r.get(2)?,
        member_ids: r.get(3)?,
    }))?
    .filter_map(|r| r.ok())
    .collect();
    Ok(results)
}

pub fn get_community_members(&self, id: i64) -> SqlResult<Vec<SearchResult>> {
    let member_json: String = self.conn.query_row(
        "SELECT member_ids FROM communities WHERE id=?1",
        params![id],
        |r| r.get(0),
    )?;
    let ids: Vec<i64> = serde_json::from_str(&member_json).unwrap_or_default();
    self.get_nodes_by_ids(&ids)
}

pub fn clear_communities(&self) -> SqlResult<()> {
    self.conn.execute("DELETE FROM communities", [])?;
    Ok(())
}
```

- [ ] **Step 5: Run tests**

```bash
cd src-tauri && cargo test graph::store -- --nocapture 2>&1 | tail -10
```

Expected: all 18 tests pass (15 original + 3 new).

- [ ] **Step 6: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/store.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p5): entities + communities tables and CRUD methods"
```

---

## Task 2: llm.rs — expose call_llm_raw

**Files:**
- Modify: `src-tauri/src/graph/llm.rs`

- [ ] **Step 1: Make call_llm publicly accessible**

In `src-tauri/src/graph/llm.rs`, change `async fn call_llm` to `pub async fn call_llm_raw` and add a shim so existing internal usages still work:

Find:
```rust
async fn call_llm(config: &LlmConfig, prompt: &str) -> Result<String, String> {
```

Replace with:
```rust
pub async fn call_llm_raw(config: &LlmConfig, prompt: &str) -> Result<String, String> {
```

Then find where `call_llm` is called inside `nl_to_query` and update it:
```rust
    let raw = call_llm_raw(config, &prompt).await?;
```

- [ ] **Step 2: Verify build**

```bash
cd src-tauri && cargo build --lib 2>&1 | grep "^error" | head -5
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/llm.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p5): expose call_llm_raw for use by community module"
```

---

## Task 3: community.rs — entity extraction + community detection + summarization + global query

**Files:**
- Create: `src-tauri/src/graph/community.rs`

- [ ] **Step 1: Write failing tests first**

Create the file with just the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_ts_entities() {
        let src = "export function authenticate(user: User) {}\nexport class TokenService {}";
        let entities = extract_code_entities(src, ".ts");
        let names: Vec<&str> = entities.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"authenticate"));
        assert!(names.contains(&"TokenService"));
    }

    #[test]
    fn extract_rs_entities() {
        let src = "pub fn process_request(req: Request) {}\npub struct Handler {}";
        let entities = extract_code_entities(src, ".rs");
        let names: Vec<&str> = entities.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"process_request"));
        assert!(names.contains(&"Handler"));
    }

    #[test]
    fn label_propagation_clusters_related_nodes() {
        // Nodes 1,2,3 share entity "auth"; nodes 4,5 share entity "payment"
        let mut node_entities: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
        node_entities.insert(1, vec!["auth".into(), "login".into()]);
        node_entities.insert(2, vec!["auth".into(), "logout".into()]);
        node_entities.insert(3, vec!["auth".into(), "token".into()]);
        node_entities.insert(4, vec!["payment".into(), "stripe".into()]);
        node_entities.insert(5, vec!["payment".into(), "invoice".into()]);

        let labels = label_propagation(&node_entities);
        // Nodes 1,2,3 should have the same label
        assert_eq!(labels[&1], labels[&2]);
        assert_eq!(labels[&2], labels[&3]);
        // Nodes 4,5 should have the same label
        assert_eq!(labels[&4], labels[&5]);
        // Auth cluster should differ from payment cluster
        assert_ne!(labels[&1], labels[&4]);
    }

    #[test]
    fn unknown_extension_returns_empty() {
        let entities = extract_code_entities("some content", ".exe");
        assert!(entities.is_empty());
    }
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cd src-tauri && cargo test graph::community -- --nocapture 2>&1 | tail -5
```

Expected: compile error (module doesn't exist).

- [ ] **Step 3: Implement community.rs**

Create `src-tauri/src/graph/community.rs` with the full implementation:

```rust
use std::collections::HashMap;
use regex::Regex;
use std::sync::OnceLock;

use crate::graph::store::{Store, CommunitySummary, SearchResult};
use crate::graph::llm::{LlmConfig, call_llm_raw};

// ─── Entity extraction ────────────────────────────────────────────────────────

static RE_JS_FN:    OnceLock<Regex> = OnceLock::new();
static RE_JS_CLASS: OnceLock<Regex> = OnceLock::new();
static RE_PY_DEF:   OnceLock<Regex> = OnceLock::new();
static RE_PY_CLASS: OnceLock<Regex> = OnceLock::new();
static RE_RS_FN:    OnceLock<Regex> = OnceLock::new();
static RE_RS_STRUCT:OnceLock<Regex> = OnceLock::new();
static RE_GO_FN:    OnceLock<Regex> = OnceLock::new();

fn re(p: &str) -> Regex { Regex::new(p).expect("bad regex") }

pub fn extract_code_entities(text: &str, extension: &str) -> Vec<(String, String)> {
    match extension {
        ".ts" | ".tsx" | ".js" | ".jsx" | ".mjs" => {
            let mut out = Vec::new();
            let fn_re = RE_JS_FN.get_or_init(|| re(r"(?m)(?:export\s+)?(?:async\s+)?function\s+(\w+)"));
            for cap in fn_re.captures_iter(text) { out.push((cap[1].to_string(), "function".into())); }
            let cls_re = RE_JS_CLASS.get_or_init(|| re(r"(?m)(?:export\s+)?class\s+(\w+)"));
            for cap in cls_re.captures_iter(text) { out.push((cap[1].to_string(), "class".into())); }
            out
        }
        ".py" => {
            let mut out = Vec::new();
            let def_re = RE_PY_DEF.get_or_init(|| re(r"(?m)^def\s+(\w+)"));
            for cap in def_re.captures_iter(text) { out.push((cap[1].to_string(), "function".into())); }
            let cls_re = RE_PY_CLASS.get_or_init(|| re(r"(?m)^class\s+(\w+)"));
            for cap in cls_re.captures_iter(text) { out.push((cap[1].to_string(), "class".into())); }
            out
        }
        ".rs" => {
            let mut out = Vec::new();
            let fn_re = RE_RS_FN.get_or_init(|| re(r"(?m)pub\s+(?:async\s+)?fn\s+(\w+)"));
            for cap in fn_re.captures_iter(text) { out.push((cap[1].to_string(), "function".into())); }
            let st_re = RE_RS_STRUCT.get_or_init(|| re(r"(?m)pub\s+struct\s+(\w+)"));
            for cap in st_re.captures_iter(text) { out.push((cap[1].to_string(), "struct".into())); }
            out
        }
        ".go" => {
            let mut out = Vec::new();
            let fn_re = RE_GO_FN.get_or_init(|| re(r"(?m)^func\s+(?:\(\w+\s+\*?\w+\)\s+)?(\w+)"));
            for cap in fn_re.captures_iter(text) { out.push((cap[1].to_string(), "function".into())); }
            out
        }
        _ => vec![],
    }
}

// ─── Entity indexing ──────────────────────────────────────────────────────────

/// Walk all FTS-indexed files, extract entities, store in DB.
pub fn index_all_entities(store: &Store) {
    let all: Vec<(i64, String, String)> = {
        let mut stmt = match store.conn.prepare(r#"
            SELECT m.node_id, n.extension, f.content
            FROM fts_node_map m
            JOIN nodes n ON n.id = m.node_id
            JOIN fts_content f ON f.rowid = m.rowid
        "#) {
            Ok(s) => s,
            Err(_) => return,
        };
        match stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default(), r.get(2)?))) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(_) => return,
        }
    };

    let _ = store.conn.execute("BEGIN", []);
    for (node_id, ext, content) in &all {
        let entities = extract_code_entities(content, ext);
        if !entities.is_empty() {
            let _ = store.insert_entities(*node_id, &entities);
        }
    }
    let _ = store.conn.execute("COMMIT", []);
}

// ─── Community detection (label propagation) ──────────────────────────────────

pub fn label_propagation(node_entities: &HashMap<i64, Vec<String>>) -> HashMap<i64, usize> {
    // Build entity → nodes reverse index
    let mut entity_to_nodes: HashMap<&str, Vec<i64>> = HashMap::new();
    for (node_id, entities) in node_entities {
        for entity in entities {
            entity_to_nodes.entry(entity.as_str()).or_default().push(*node_id);
        }
    }

    // Build weighted adjacency: node → (neighbor, shared_entity_count)
    let mut adjacency: HashMap<i64, HashMap<i64, f32>> = HashMap::new();
    for nodes in entity_to_nodes.values().filter(|v| v.len() > 1 && v.len() < 50) {
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                *adjacency.entry(nodes[i]).or_default().entry(nodes[j]).or_insert(0.0) += 1.0;
                *adjacency.entry(nodes[j]).or_default().entry(nodes[i]).or_insert(0.0) += 1.0;
            }
        }
    }

    // Initialize: each node is its own community
    let mut labels: HashMap<i64, usize> = node_entities.keys()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    // Propagate labels (max 20 iterations)
    let mut all_nodes: Vec<i64> = node_entities.keys().copied().collect();
    all_nodes.sort_unstable(); // deterministic order
    for _ in 0..20 {
        let mut changed = false;
        for &node in &all_nodes {
            if let Some(neighbors) = adjacency.get(&node) {
                let mut votes: HashMap<usize, f32> = HashMap::new();
                for (&neighbor, &weight) in neighbors {
                    *votes.entry(labels[&neighbor]).or_insert(0.0) += weight;
                }
                if let Some((&best, _)) = votes.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal)) {
                    if labels[&node] != best {
                        labels.insert(node, best);
                        changed = true;
                    }
                }
            }
        }
        if !changed { break; }
    }

    // Normalize label IDs to 0..N
    let mut unique: Vec<usize> = labels.values().copied().collect::<std::collections::HashSet<_>>().into_iter().collect();
    unique.sort_unstable();
    let remap: HashMap<usize, usize> = unique.iter().enumerate().map(|(i, &l)| (l, i)).collect();
    labels.into_iter().map(|(node, label)| (node, remap[&label])).collect()
}

/// Build communities from entity co-occurrence and store them.
pub fn rebuild_communities(store: &Store) {
    // Extract entities first
    index_all_entities(store);

    // Get all node entities
    let node_entities_raw = match store.get_all_node_entities() {
        Ok(v) => v,
        Err(_) => return,
    };
    if node_entities_raw.is_empty() { return; }

    let node_entities: HashMap<i64, Vec<String>> = node_entities_raw.into_iter().collect();

    // Detect communities
    let labels = label_propagation(&node_entities);

    // Group nodes by community label
    let mut by_label: HashMap<usize, Vec<i64>> = HashMap::new();
    for (node_id, label) in &labels {
        by_label.entry(*label).or_default().push(*node_id);
    }

    // Write communities (clear old ones first)
    let _ = store.clear_communities();
    let _ = store.conn.execute("BEGIN", []);
    for (_, members) in &by_label {
        if members.len() >= 2 { // skip singleton communities
            let _ = store.upsert_community(None, None, members);
        }
    }
    let _ = store.conn.execute("COMMIT", []);
}

// ─── Community summarization ──────────────────────────────────────────────────

pub async fn summarize_communities(store: &Store, config: &LlmConfig) {
    let communities = match store.list_communities() {
        Ok(c) => c,
        Err(_) => return,
    };

    for community in &communities {
        if community.summary.is_some() { continue; } // skip already summarized

        let members = match store.get_community_members(community.id) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let file_list = members.iter().take(10)
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        let ids: Vec<i64> = serde_json::from_str(&community.member_ids).unwrap_or_default();
        let entity_names: Vec<String> = ids.iter()
            .filter_map(|&id| store.get_entity_names_for_node(id).ok())
            .flatten()
            .take(20)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let prompt = format!(
            "In one sentence, summarize what this group of code files is about.\nFiles: {}\nKey symbols: {}\nSummary:",
            file_list,
            entity_names.join(", ")
        );

        match call_llm_raw(config, &prompt).await {
            Ok(summary) => { let _ = store.update_community_summary(community.id, summary.trim()); }
            Err(e) => eprintln!("[community] summarize failed for {}: {e}", community.id),
        }
    }
}

// ─── GraphRAG global query ────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GlobalAnswer {
    pub answer:           String,
    pub sources:          Vec<SearchResult>,
    pub communities_used: Vec<i64>,
}

pub async fn global_query(
    question: &str,
    store: &Store,
    config: &LlmConfig,
) -> Result<GlobalAnswer, String> {
    let communities = store.list_communities().map_err(|e| e.to_string())?;
    if communities.is_empty() {
        return Err("No communities built yet. Run graph_rebuild_communities first.".to_string());
    }

    // Step 1: map question → relevant community IDs using summaries
    let summarized: Vec<&CommunitySummary> = communities.iter()
        .filter(|c| c.summary.is_some())
        .collect();

    let relevant_ids: Vec<i64> = if summarized.is_empty() {
        // No summaries yet — use all communities
        communities.iter().map(|c| c.id).take(3).collect()
    } else {
        let summaries_ctx = summarized.iter()
            .map(|c| format!("ID {}: {}", c.id, c.summary.as_deref().unwrap_or("")))
            .collect::<Vec<_>>()
            .join("\n");

        let map_prompt = format!(
            "These are summaries of code communities:\n{}\n\nWhich community IDs are most relevant to answer: \"{}\"\nReply with ONLY comma-separated numbers, e.g.: 1,3",
            summaries_ctx, question
        );

        match call_llm_raw(config, &map_prompt).await {
            Ok(raw) => raw.split(',').filter_map(|s| s.trim().parse().ok()).collect(),
            Err(_)  => communities.iter().map(|c| c.id).take(3).collect(),
        }
    };

    // Step 2: collect member files from matched communities
    let member_node_ids: Vec<i64> = relevant_ids.iter()
        .filter_map(|&id| {
            communities.iter().find(|c| c.id == id)
                .and_then(|c| serde_json::from_str::<Vec<i64>>(&c.member_ids).ok())
        })
        .flatten()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let sources = store.get_nodes_by_ids(&member_node_ids).map_err(|e| e.to_string())?;

    // Step 3: generate answer using file names + community context as grounding
    let context = sources.iter().take(8)
        .map(|r| format!("- {}", r.name))
        .collect::<Vec<_>>()
        .join("\n");

    let answer_prompt = format!(
        "You are analyzing a codebase. Based on these relevant files:\n{}\n\nAnswer this question: {}\n\nBe concise and specific.",
        context, question
    );

    let answer = call_llm_raw(config, &answer_prompt).await
        .unwrap_or_else(|_| "Unable to generate answer — LLM unavailable.".to_string());

    Ok(GlobalAnswer {
        answer,
        sources: sources.into_iter().take(10).collect(),
        communities_used: relevant_ids,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_ts_entities() {
        let src = "export function authenticate(user: User) {}\nexport class TokenService {}";
        let entities = extract_code_entities(src, ".ts");
        let names: Vec<&str> = entities.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"authenticate"));
        assert!(names.contains(&"TokenService"));
    }

    #[test]
    fn extract_rs_entities() {
        let src = "pub fn process_request() {}\npub struct Handler {}";
        let entities = extract_code_entities(src, ".rs");
        let names: Vec<&str> = entities.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"process_request"));
        assert!(names.contains(&"Handler"));
    }

    #[test]
    fn label_propagation_clusters_related_nodes() {
        let mut node_entities: HashMap<i64, Vec<String>> = HashMap::new();
        node_entities.insert(1, vec!["auth".into(), "login".into()]);
        node_entities.insert(2, vec!["auth".into(), "logout".into()]);
        node_entities.insert(3, vec!["auth".into(), "token".into()]);
        node_entities.insert(4, vec!["payment".into(), "stripe".into()]);
        node_entities.insert(5, vec!["payment".into(), "invoice".into()]);

        let labels = label_propagation(&node_entities);
        assert_eq!(labels[&1], labels[&2]);
        assert_eq!(labels[&2], labels[&3]);
        assert_eq!(labels[&4], labels[&5]);
        assert_ne!(labels[&1], labels[&4]);
    }

    #[test]
    fn unknown_extension_returns_empty() {
        let entities = extract_code_entities("some content", ".exe");
        assert!(entities.is_empty());
    }
}
```

- [ ] **Step 4: Add `pub mod community;` to mod.rs**

Add after `pub mod embedder;` in `src-tauri/src/graph/mod.rs`:
```rust
pub mod community;
```

- [ ] **Step 5: Run tests**

```bash
cd src-tauri && cargo test graph::community -- --nocapture 2>&1 | tail -15
```

Expected: 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/community.rs src-tauri/src/graph/mod.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p5): community.rs — entity extraction, label propagation, GraphRAG query"
```

---

## Task 4: 4 new Tauri commands

**Files:**
- Modify: `src-tauri/src/graph/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add Community + CommunityDetail types and 4 commands to mod.rs**

Add after `graph_embed_folder`:

```rust
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
    let store_arc     = state.store.clone();
    let llm_config    = state.llm_config.lock().map_err(|e| e.to_string())?.clone();

    tauri::async_runtime::spawn(async move {
        // Run sync community detection
        if let Ok(store) = store_arc.lock() {
            community::rebuild_communities(&store);
        }
        // Async summarization if LLM configured
        if let Some(config) = llm_config {
            if let Ok(store) = store_arc.lock() {
                community::summarize_communities(&store, &config).await;
            }
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
    Ok(CommunityDetail {
        id: c.id,
        label: c.label,
        summary: c.summary,
        members,
    })
}

#[tauri::command]
pub async fn graph_global_query(
    question: String,
    state: State<'_, GraphAppState>,
) -> Result<community::GlobalAnswer, String> {
    let config = state.llm_config.lock().map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "No LLM configured. Set one via graph_set_llm.".to_string())?;

    let store = state.store.lock().map_err(|e| e.to_string())?;
    community::global_query(&question, &store, &config).await
}
```

- [ ] **Step 2: Register in lib.rs**

Add after `graph::graph_embed_folder`:
```rust
graph::graph_rebuild_communities,
graph::graph_list_communities,
graph::graph_get_community,
graph::graph_global_query,
```

- [ ] **Step 3: Build and test**

```bash
cd src-tauri && cargo build --lib 2>&1 | grep "^error" | head -10
cd src-tauri && cargo test --lib 2>&1 | tail -5
```

Expected: no errors, all tests pass.

- [ ] **Step 4: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/mod.rs src-tauri/src/lib.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p5): graph_global_query, graph_list_communities, graph_get_community, graph_rebuild_communities"
```

---

## Task 5: Frontend — Ask AI mode + Answer panel + Communities

**Files:**
- Modify: `frontend/js/graph.ts`
- Modify: `frontend/js/graphui.ts`
- Modify: `frontend/index.html`
- Modify: `frontend/css/style.css`

- [ ] **Step 1: Add API bindings to graph.ts** (after `graphEmbedFolder`):

```typescript
export interface GlobalAnswer {
    answer:            string;
    sources:           GraphSearchResult[];
    communities_used:  number[];
}

export interface Community {
    id:      number;
    label?:  string;
    summary?: string;
    size:    number;
}

export interface CommunityDetail {
    id:      number;
    label?:  string;
    summary?: string;
    members: GraphSearchResult[];
}

export async function graphGlobalQuery(question: string): Promise<GlobalAnswer> {
    return _invoke('graph_global_query', { question }) as Promise<GlobalAnswer>;
}

export async function graphListCommunities(): Promise<Community[]> {
    return _invoke('graph_list_communities') as Promise<Community[]>;
}

export async function graphGetCommunity(id: number): Promise<CommunityDetail> {
    return _invoke('graph_get_community', { id }) as Promise<CommunityDetail>;
}

export async function graphRebuildCommunities(): Promise<void> {
    return _invoke('graph_rebuild_communities') as Promise<void>;
}
```

- [ ] **Step 2: Update graphui.ts — wire Ask AI mode + communities panel**

In the `_runSearch` function, change the `ask` branch to call `graphGlobalQuery`:

Find in `graphui.ts` the `_bindModeButtons` function and replace the submit handler:
```typescript
    document.getElementById('graph-search-form')?.addEventListener('submit', async (e) => {
        e.preventDefault();
        const input = document.getElementById('graph-search-input') as HTMLInputElement;
        const query = input?.value.trim();
        if (!query) return;
        if (_currentMode === 'search') await _runSearch(query);
        else if (_currentMode === 'ask') await _runAskAI(query);
    });
```

Add `_runAskAI` function (add after `_runSearch`):

```typescript
async function _runAskAI(question: string): Promise<void> {
    const panel = document.getElementById('graph-results-panel')!;
    panel.classList.remove('hidden');
    panel.innerHTML = '<div class="graph-results-loading">Asking AI…</div>';

    try {
        const answer = await graphApi.graphGlobalQuery(question);
        const sourceItems = answer.sources.map(r => `
            <div class="graph-result-item" data-path="${_escHtml(r.path)}" title="${_escHtml(r.path)}">
                <span class="gr-icon">📄</span>
                <span class="gr-name">${_escHtml(r.name)}</span>
                <span class="gr-size">${r.size_human}</span>
            </div>
        `).join('');

        panel.innerHTML = `
            <div class="graph-results-header">
                <span>AI Answer</span>
                <button id="graph-results-close" class="graph-results-close">✕</button>
            </div>
            <div class="graph-answer-text">${_escHtml(answer.answer)}</div>
            ${answer.sources.length ? `
                <div class="graph-answer-sources-label">Sources (${answer.sources.length} files)</div>
                <div class="graph-results-list">${sourceItems}</div>
            ` : ''}
        `;
        document.getElementById('graph-results-close')?.addEventListener('click', hideResultsPanel);
    } catch (e) {
        const msg = String(e);
        panel.innerHTML = `<div class="graph-results-empty">${_escHtml(msg.includes('No LLM') ? 'Set an LLM provider first (graph_set_llm command).' : msg)}</div>`;
    }
}
```

Add `refreshCommunitiesList` function (add after `refreshIndexedFoldersList`):

```typescript
export async function refreshCommunitiesList(): Promise<void> {
    const list = document.getElementById('graph-communities-list');
    if (!list) return;

    let communities: import('./graph.js').Community[] = [];
    try { communities = await graphApi.graphListCommunities(); } catch { return; }

    list.innerHTML = communities.length === 0
        ? '<div class="graph-no-folders">No communities detected yet.<br><small>Deep-index a folder first, then rebuild communities.</small></div>'
        : communities.map(c => `
            <div class="graph-community-item">
                <span class="gc-summary">${_escHtml(c.summary || `Community ${c.id}`)}</span>
                <span class="gc-size">${c.size} files</span>
            </div>
          `).join('');
}
```

Add to `initGraphUI`, after the `graph-embeddings-ready` listener:
```typescript
    refreshCommunitiesList();
    if (tauri?.event?.listen) {
        tauri.event.listen('graph-communities-ready', () => {
            refreshCommunitiesList();
            toast('Communities rebuilt', 'success');
        });
    }
```

- [ ] **Step 3: Add HTML to index.html**

Add a communities section inside `#graph-indexed-panel` (after the `graph-indexed-folders-list` div):

```html
<!-- Communities section inside the indexed panel -->
<div class="graph-indexed-header" style="margin-top:8px">
    <span>Detected Communities</span>
    <button id="btn-rebuild-communities" style="background:none;border:none;color:var(--accent);cursor:pointer;font-size:0.72rem;">Rebuild</button>
</div>
<div id="graph-communities-list"></div>
```

- [ ] **Step 4: Add CSS to style.css** (append to end):

```css
/* ── AI answer panel ────────────────────────────────────────────── */
.graph-answer-text {
    padding: 12px 14px;
    font-size: 0.82rem;
    line-height: 1.6;
    border-bottom: 1px solid var(--border);
    white-space: pre-wrap;
}

.graph-answer-sources-label {
    padding: 6px 12px;
    font-size: 0.72rem;
    color: var(--text-dim);
    border-bottom: 1px solid var(--border);
}

/* ── Communities list ───────────────────────────────────────────── */
.graph-community-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 12px;
    font-size: 0.78rem;
    border-bottom: 1px solid var(--border);
}

.gc-summary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
}

.gc-size {
    font-size: 0.7rem;
    color: var(--text-dim);
    flex-shrink: 0;
    margin-left: 8px;
}
```

- [ ] **Step 5: Wire Rebuild button in events.ts**

In `bindGlobalEvents()`, add:
```typescript
    document.getElementById('btn-rebuild-communities')?.addEventListener('click', async () => {
        toast('Rebuilding communities…', '');
        try {
            await graphApi.graphRebuildCommunities();
        } catch (e) {
            toast(`Rebuild failed: ${e}`, 'error');
        }
    });
```

Add `graphApi` import in events.ts — find the existing graphui import and check if graph.ts is imported directly. If not, add:
```typescript
import * as graphApi from './graph.js';
```

- [ ] **Step 6: Compile**

```bash
npm run build:ts 2>&1
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git -C /Users/suraj/Documents/tracer add frontend/js/graph.ts frontend/js/graph.js frontend/js/graphui.ts frontend/js/graphui.js frontend/index.html frontend/css/style.css frontend/js/events.ts frontend/js/events.js
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p5): Ask AI mode, answer panel, communities panel"
```

---

## Task 6: Final Verification

- [ ] **Step 1: Full test suite**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -8
```

Expected: all 62+ tests pass.

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

- [ ] **Step 4: Smoke test (requires ollama)**

```bash
npm run tauri dev
```

Test sequence:
1. Right-click a code folder → "Deep Index This Folder"
2. Wait for "Content indexing complete" toast
3. Click "Rebuild" button in the indexed panel → "Communities rebuilt" toast
4. Communities list shows detected clusters with summaries (if LLM configured)
5. Switch to "Ask AI" tab → type "What does this codebase do?" → answer panel shows LLM response + source files
6. Without LLM: Ask AI shows helpful error "Set an LLM provider first"

- [ ] **Step 5: Final commit**

```bash
git -C /Users/suraj/Documents/tracer add -A
git -C /Users/suraj/Documents/tracer commit -m "feat: knowledge graph Phase 5 complete — GraphRAG community intelligence" --allow-empty
```
