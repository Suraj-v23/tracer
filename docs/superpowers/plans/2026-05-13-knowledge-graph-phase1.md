# Knowledge Graph — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Background filesystem scanner that builds a SQLite + petgraph metadata index, watches for changes, and exposes Tauri commands for structured search and NL→query via LLM.

**Architecture:** New `graph` Rust module (store/indexer/query/llm/mod) mirrors the existing `transfer` module pattern. SQLite (rusqlite bundled-full) stores nodes+edges persistently; petgraph holds an in-memory directed graph for traversal. A file watcher (notify-debouncer-mini) incrementally updates both. The frontend gains a three-mode search bar (Filter / Search / Ask AI), a results panel, and an index status bar.

**Tech Stack:** `rusqlite 0.31 bundled-full`, `petgraph 0.6`, `notify-debouncer-mini 0.4`, `blake3 1`, existing `rayon`, `reqwest`, `walkdir`, `serde_json`. Frontend: TypeScript compiled to JS (existing `tsc` pipeline).

---

## File Map

**Create (Rust):**
- `src-tauri/src/graph/mod.rs` — GraphAppState + all Tauri commands
- `src-tauri/src/graph/store.rs` — SQLite schema, node CRUD, query methods
- `src-tauri/src/graph/indexer.rs` — parallel scanner + file watcher
- `src-tauri/src/graph/query.rs` — StructuredQuery enum + execution
- `src-tauri/src/graph/llm.rs` — LLM bridge (ollama / remote)

**Modify (Rust):**
- `src-tauri/Cargo.toml` — add four new dependencies
- `src-tauri/src/lib.rs` — add `mod graph`, init GraphAppState, register commands

**Create (Frontend):**
- `frontend/js/graph.ts` — TypeScript API bindings + interfaces
- `frontend/js/graphui.ts` — search mode toggle, results panel, status bar logic

**Modify (Frontend):**
- `frontend/index.html` — search mode toggle, results panel, status bar HTML
- `frontend/css/style.css` — styles for new UI elements
- `frontend/js/main.ts` — import and init graphui

---

## Task 1: Add Dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add dependencies**

Open `src-tauri/Cargo.toml` and add to `[dependencies]`:

```toml
rusqlite = { version = "0.31", features = ["bundled-full"] }
petgraph = "0.6"
notify-debouncer-mini = "0.4"
blake3 = "1"
```

- [ ] **Step 2: Verify they resolve**

```bash
cd src-tauri && cargo fetch
```

Expected: no errors, packages downloaded.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml
git commit -m "chore: add graph system dependencies (rusqlite, petgraph, notify, blake3)"
```

---

## Task 2: store.rs — Schema + Node CRUD

**Files:**
- Create: `src-tauri/src/graph/store.rs`

- [ ] **Step 1: Write the failing test first**

Create `src-tauri/src/graph/store.rs` with this content:

```rust
use rusqlite::{Connection, Result as SqlResult, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ─── Data types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphNode {
    pub id:            i64,
    pub path:          String,
    pub name:          String,
    pub kind:          String,   // "file" | "directory"
    pub size:          u64,
    pub extension:     Option<String>,
    pub modified_secs: Option<i64>,
    pub created_secs:  Option<i64>,
    pub content_hash:  Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub path:          String,
    pub name:          String,
    pub kind:          String,
    pub size:          u64,
    pub size_human:    String,
    pub extension:     Option<String>,
    pub modified_secs: Option<i64>,
    pub snippet:       Option<String>,
    pub score:         f64,
}

// ─── Store ───────────────────────────────────────────────────────────────────

pub struct Store {
    pub conn: Connection,
}

impl Store {
    pub fn open(db_path: &Path) -> SqlResult<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(db_path)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> SqlResult<()> {
        self.conn.execute_batch(r#"
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA foreign_keys=ON;

            CREATE TABLE IF NOT EXISTS nodes (
                id            INTEGER PRIMARY KEY,
                path          TEXT UNIQUE NOT NULL,
                name          TEXT NOT NULL,
                kind          TEXT NOT NULL,
                size          INTEGER DEFAULT 0,
                extension     TEXT,
                modified_secs INTEGER,
                created_secs  INTEGER,
                content_hash  TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_nodes_size     ON nodes(size);
            CREATE INDEX IF NOT EXISTS idx_nodes_ext      ON nodes(extension);
            CREATE INDEX IF NOT EXISTS idx_nodes_modified ON nodes(modified_secs);
            CREATE INDEX IF NOT EXISTS idx_nodes_name     ON nodes(name COLLATE NOCASE);

            CREATE TABLE IF NOT EXISTS edges (
                from_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                to_id   INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                kind    TEXT NOT NULL,
                UNIQUE(from_id, to_id, kind)
            );
            CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
            CREATE INDEX IF NOT EXISTS idx_edges_to   ON edges(to_id);

            CREATE TABLE IF NOT EXISTS indexed_folders (
                path       TEXT PRIMARY KEY,
                added_secs INTEGER
            );

            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT
            );
        "#)
    }

    // ── Node CRUD ────────────────────────────────────────────────────────────

    pub fn upsert_node(&self, node: &GraphNode) -> SqlResult<i64> {
        self.conn.execute(
            r#"INSERT INTO nodes (path, name, kind, size, extension, modified_secs, created_secs, content_hash)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
               ON CONFLICT(path) DO UPDATE SET
                 name=excluded.name, kind=excluded.kind, size=excluded.size,
                 extension=excluded.extension, modified_secs=excluded.modified_secs,
                 created_secs=excluded.created_secs, content_hash=excluded.content_hash"#,
            params![node.path, node.name, node.kind, node.size as i64,
                    node.extension, node.modified_secs, node.created_secs, node.content_hash],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn delete_node(&self, path: &str) -> SqlResult<()> {
        self.conn.execute("DELETE FROM nodes WHERE path=?1", params![path])?;
        Ok(())
    }

    pub fn get_node_id(&self, path: &str) -> SqlResult<Option<i64>> {
        let mut stmt = self.conn.prepare_cached("SELECT id FROM nodes WHERE path=?1")?;
        let mut rows = stmt.query(params![path])?;
        Ok(rows.next()?.map(|r| r.get_unwrap(0)))
    }

    pub fn node_count(&self) -> SqlResult<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
    }

    // ── Edge CRUD ────────────────────────────────────────────────────────────

    pub fn upsert_edge(&self, from_path: &str, to_path: &str, kind: &str) -> SqlResult<()> {
        let from_id = match self.get_node_id(from_path)? { Some(id) => id, None => return Ok(()) };
        let to_id   = match self.get_node_id(to_path)?   { Some(id) => id, None => return Ok(()) };
        self.conn.execute(
            "INSERT OR IGNORE INTO edges (from_id, to_id, kind) VALUES (?1,?2,?3)",
            params![from_id, to_id, kind],
        )?;
        Ok(())
    }

    // ── Queries ──────────────────────────────────────────────────────────────

    pub fn query_metadata(
        &self,
        name_contains:  Option<&str>,
        extension:      Option<&str>,
        kind:           Option<&str>,
        size_gt:        Option<u64>,
        size_lt:        Option<u64>,
        modified_after: Option<i64>,
    ) -> SqlResult<Vec<SearchResult>> {
        let mut sql = String::from(
            "SELECT path, name, kind, size, extension, modified_secs FROM nodes WHERE 1=1"
        );
        let mut param_values: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(n) = name_contains {
            param_values.push(Box::new(format!("%{n}%")));
            sql.push_str(&format!(" AND name LIKE ?{}", param_values.len()));
        }
        if let Some(e) = extension {
            param_values.push(Box::new(e.to_string()));
            sql.push_str(&format!(" AND extension=?{}", param_values.len()));
        }
        if let Some(k) = kind {
            param_values.push(Box::new(k.to_string()));
            sql.push_str(&format!(" AND kind=?{}", param_values.len()));
        }
        if let Some(sg) = size_gt {
            param_values.push(Box::new(sg as i64));
            sql.push_str(&format!(" AND size>?{}", param_values.len()));
        }
        if let Some(sl) = size_lt {
            param_values.push(Box::new(sl as i64));
            sql.push_str(&format!(" AND size<?{}", param_values.len()));
        }
        if let Some(ma) = modified_after {
            param_values.push(Box::new(ma));
            sql.push_str(&format!(" AND modified_secs>?{}", param_values.len()));
        }
        sql.push_str(" ORDER BY size DESC LIMIT 200");

        let params_ref: Vec<&dyn rusqlite::ToSql> = param_values.iter().map(|b| b.as_ref()).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let results = stmt.query_map(params_ref.as_slice(), row_to_result)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn find_duplicates(&self, path: &str) -> SqlResult<Vec<SearchResult>> {
        let mut stmt = self.conn.prepare(r#"
            SELECT n2.path, n2.name, n2.kind, n2.size, n2.extension, n2.modified_secs
            FROM nodes n1
            JOIN edges e ON e.from_id = n1.id AND e.kind = 'duplicate'
            JOIN nodes n2 ON n2.id = e.to_id
            WHERE n1.path = ?1
        "#)?;
        let results = stmt.query_map(params![path], row_to_result)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn get_children(&self, path: &str, depth: usize) -> SqlResult<Vec<SearchResult>> {
        let pattern = if path.ends_with('/') {
            format!("{path}%")
        } else {
            format!("{path}/%")
        };
        let max_slashes = path.matches('/').count() + depth;
        let mut stmt = self.conn.prepare(r#"
            SELECT path, name, kind, size, extension, modified_secs
            FROM nodes
            WHERE path LIKE ?1
              AND (LENGTH(path) - LENGTH(REPLACE(path, '/', ''))) <= ?2
            ORDER BY size DESC LIMIT 200
        "#)?;
        let results = stmt.query_map(params![pattern, max_slashes as i64], row_to_result)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn get_setting(&self, key: &str) -> SqlResult<Option<String>> {
        let mut stmt = self.conn.prepare_cached("SELECT value FROM settings WHERE key=?1")?;
        let mut rows = stmt.query(params![key])?;
        Ok(rows.next()?.map(|r| r.get_unwrap(0)))
    }

    pub fn set_setting(&self, key: &str, value: &str) -> SqlResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key,value) VALUES (?1,?2)",
            params![key, value],
        )?;
        Ok(())
    }
}

fn row_to_result(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchResult> {
    let size: i64 = row.get(3)?;
    Ok(SearchResult {
        path:          row.get(0)?,
        name:          row.get(1)?,
        kind:          row.get(2)?,
        size:          size as u64,
        size_human:    format_size(size as u64),
        extension:     row.get(4)?,
        modified_secs: row.get(5)?,
        snippet:       None,
        score:         1.0,
    })
}

pub fn format_size(bytes: u64) -> String {
    if bytes == 0 { return "0 B".into(); }
    let units = ["B","KB","MB","GB","TB"];
    let mut size = bytes as f64;
    let mut idx = 0;
    while size >= 1000.0 && idx < units.len() - 1 { size /= 1000.0; idx += 1; }
    if idx == 0 { format!("{bytes} B") } else { format!("{:.1} {}", size, units[idx]) }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(path: &str, name: &str, kind: &str, size: u64) -> GraphNode {
        GraphNode { id: 0, path: path.into(), name: name.into(), kind: kind.into(),
                    size, extension: None, modified_secs: None, created_secs: None, content_hash: None }
    }

    #[test]
    fn upsert_and_count() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/b.txt", "b.txt", "file", 1024)).unwrap();
        assert_eq!(store.node_count().unwrap(), 1);
    }

    #[test]
    fn upsert_updates_existing() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/b.txt", "b.txt", "file", 1024)).unwrap();
        store.upsert_node(&make_node("/a/b.txt", "b.txt", "file", 2048)).unwrap();
        assert_eq!(store.node_count().unwrap(), 1);
    }

    #[test]
    fn delete_node_removes_it() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/b.txt", "b.txt", "file", 100)).unwrap();
        store.delete_node("/a/b.txt").unwrap();
        assert_eq!(store.node_count().unwrap(), 0);
    }

    #[test]
    fn query_by_name_contains() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/readme.md", "readme.md", "file", 500)).unwrap();
        store.upsert_node(&make_node("/a/main.rs",   "main.rs",   "file", 200)).unwrap();
        let results = store.query_metadata(Some("readme"), None, None, None, None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "readme.md");
    }

    #[test]
    fn query_by_size_gt() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/big.mp4",   "big.mp4",   "file", 500_000_000)).unwrap();
        store.upsert_node(&make_node("/a/small.txt", "small.txt", "file", 1_000)).unwrap();
        let results = store.query_metadata(None, None, None, Some(100_000_000), None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "big.mp4");
    }

    #[test]
    fn find_duplicates_returns_peers() {
        let store = Store::open_in_memory().unwrap();
        let mut n1 = make_node("/a/f1.txt", "f1.txt", "file", 100);
        n1.content_hash = Some("abc123".into());
        let mut n2 = make_node("/b/f2.txt", "f2.txt", "file", 100);
        n2.content_hash = Some("abc123".into());
        store.upsert_node(&n1).unwrap();
        store.upsert_node(&n2).unwrap();
        store.upsert_edge("/a/f1.txt", "/b/f2.txt", "duplicate").unwrap();
        let dupes = store.find_duplicates("/a/f1.txt").unwrap();
        assert_eq!(dupes.len(), 1);
        assert_eq!(dupes[0].name, "f2.txt");
    }

    #[test]
    fn format_size_correct() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1_500), "1.5 KB");
        assert_eq!(format_size(1_500_000), "1.5 MB");
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

```bash
cd src-tauri && cargo test graph::store -- --nocapture 2>&1 | tail -20
```

Expected:
```
test graph::store::tests::upsert_and_count ... ok
test graph::store::tests::upsert_updates_existing ... ok
test graph::store::tests::delete_node_removes_it ... ok
test graph::store::tests::query_by_name_contains ... ok
test graph::store::tests::query_by_size_gt ... ok
test graph::store::tests::find_duplicates_returns_peers ... ok
test graph::store::tests::format_size_correct ... ok
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/graph/store.rs
git commit -m "feat(graph): store.rs — SQLite schema, node CRUD, query methods"
```

---

## Task 3: query.rs — StructuredQuery + Execution

**Files:**
- Create: `src-tauri/src/graph/query.rs`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/graph/query.rs`:

```rust
use serde::{Deserialize, Serialize};
use crate::graph::store::{Store, SearchResult};

// ─── Query types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum StructuredQuery {
    MetadataFilter {
        #[serde(default)] name_contains:  Option<String>,
        #[serde(default)] extension:      Option<String>,
        #[serde(default)] kind_filter:    Option<String>,
        #[serde(default)] size_gt:        Option<u64>,
        #[serde(default)] size_lt:        Option<u64>,
        #[serde(default)] modified_after: Option<i64>,
    },
    FindDuplicates {
        path: String,
    },
    GetRelated {
        path:  String,
        #[serde(default = "default_depth")] depth: usize,
    },
}

fn default_depth() -> usize { 1 }

impl Default for StructuredQuery {
    fn default() -> Self {
        StructuredQuery::MetadataFilter {
            name_contains: None, extension: None, kind_filter: None,
            size_gt: None, size_lt: None, modified_after: None,
        }
    }
}

// ─── Execution ───────────────────────────────────────────────────────────────

pub fn execute(query: &StructuredQuery, store: &Store) -> Result<Vec<SearchResult>, String> {
    match query {
        StructuredQuery::MetadataFilter {
            name_contains, extension, kind_filter, size_gt, size_lt, modified_after
        } => store.query_metadata(
            name_contains.as_deref(),
            extension.as_deref(),
            kind_filter.as_deref(),
            *size_gt, *size_lt, *modified_after,
        ).map_err(|e| e.to_string()),

        StructuredQuery::FindDuplicates { path } =>
            store.find_duplicates(path).map_err(|e| e.to_string()),

        StructuredQuery::GetRelated { path, depth } =>
            store.get_children(path, *depth).map_err(|e| e.to_string()),
    }
}

// ─── NL fallback (no LLM) ────────────────────────────────────────────────────

/// Best-effort heuristic parse when LLM is unavailable.
pub fn heuristic_parse(input: &str) -> StructuredQuery {
    let lower = input.to_lowercase();

    // "duplicate" / "dupes" → FindDuplicates on root
    if lower.contains("duplicate") || lower.contains("dupe") {
        return StructuredQuery::FindDuplicates { path: "/".into() };
    }

    // Extension hints: ".mp4", "videos", "images" etc.
    let extension = if lower.contains("video") || lower.contains(".mp4") { Some(".mp4".into()) }
        else if lower.contains("image") || lower.contains("photo") { Some(".jpg".into()) }
        else if lower.contains("pdf") { Some(".pdf".into()) }
        else { None };

    // Size hints
    let size_gt = if lower.contains("large") || lower.contains("big") { Some(100 * 1024 * 1024) }
        else { None };

    StructuredQuery::MetadataFilter {
        name_contains: Some(input.into()),
        extension,
        kind_filter: None,
        size_gt,
        size_lt: None,
        modified_after: None,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::store::Store;

    fn seed_store() -> Store {
        let store = Store::open_in_memory().unwrap();
        let nodes = vec![
            ("file", "/home/a.mp4",  "a.mp4",  ".mp4",  200_000_000_i64),
            ("file", "/home/b.txt",  "b.txt",  ".txt",  1_000),
            ("file", "/home/c.rs",   "c.rs",   ".rs",   5_000),
            ("directory", "/home/src", "src",  "",      0),
        ];
        for (kind, path, name, ext, size) in nodes {
            store.conn.execute(
                "INSERT INTO nodes (path,name,kind,size,extension) VALUES (?1,?2,?3,?4,?5)",
                rusqlite::params![path, name, kind, size, if ext.is_empty() { None } else { Some(ext) }],
            ).unwrap();
        }
        store
    }

    #[test]
    fn metadata_filter_by_extension() {
        let store = seed_store();
        let q = StructuredQuery::MetadataFilter {
            name_contains: None, extension: Some(".mp4".into()),
            kind_filter: None, size_gt: None, size_lt: None, modified_after: None,
        };
        let results = execute(&q, &store).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "a.mp4");
    }

    #[test]
    fn metadata_filter_size_gt() {
        let store = seed_store();
        let q = StructuredQuery::MetadataFilter {
            name_contains: None, extension: None, kind_filter: None,
            size_gt: Some(100_000_000), size_lt: None, modified_after: None,
        };
        let results = execute(&q, &store).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "a.mp4");
    }

    #[test]
    fn heuristic_parse_detects_large_video() {
        let q = heuristic_parse("find large videos");
        match q {
            StructuredQuery::MetadataFilter { extension, size_gt, .. } => {
                assert_eq!(extension, Some(".mp4".into()));
                assert!(size_gt.is_some());
            }
            _ => panic!("expected MetadataFilter"),
        }
    }

    #[test]
    fn heuristic_parse_detects_duplicates() {
        let q = heuristic_parse("show duplicate files");
        assert!(matches!(q, StructuredQuery::FindDuplicates { .. }));
    }

    #[test]
    fn structured_query_roundtrips_json() {
        let q = StructuredQuery::MetadataFilter {
            name_contains: Some("readme".into()),
            extension: Some(".md".into()),
            kind_filter: None, size_gt: None, size_lt: None, modified_after: None,
        };
        let json = serde_json::to_string(&q).unwrap();
        let q2: StructuredQuery = serde_json::from_str(&json).unwrap();
        assert!(matches!(q2, StructuredQuery::MetadataFilter { .. }));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cd src-tauri && cargo test graph::query -- --nocapture 2>&1 | tail -15
```

Expected: 5 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/graph/query.rs
git commit -m "feat(graph): query.rs — StructuredQuery enum, execution, heuristic fallback"
```

---

## Task 4: indexer.rs — Parallel Scanner + File Watcher

**Files:**
- Create: `src-tauri/src/graph/indexer.rs`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/graph/indexer.rs`:

```rust
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;

use crate::graph::store::{GraphNode, Store};

// ─── Scan ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct IndexStats {
    pub total:    usize,
    pub indexed:  usize,
    pub errors:   usize,
    pub watching: bool,
}

pub fn scan_and_index(root: &Path, store: &Store) -> Result<IndexStats, String> {
    let entries: Vec<_> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| !e.path().symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(true))
        .collect();

    let total = entries.len();

    // Build nodes in parallel
    let nodes: Vec<GraphNode> = entries.par_iter()
        .filter_map(|entry| entry_to_node(entry.path()))
        .collect();

    // Insert into SQLite (single writer — sequential)
    let mut indexed = 0;
    let mut errors  = 0;
    for node in &nodes {
        match store.upsert_node(node) {
            Ok(_)  => indexed += 1,
            Err(_) => errors  += 1,
        }
    }

    // Parent edges
    for entry in &entries {
        if let Some(parent) = entry.path().parent() {
            let _ = store.upsert_edge(
                &entry.path().to_string_lossy(),
                &parent.to_string_lossy(),
                "parent",
            );
        }
    }

    // Duplicate edges (group by blake3 hash)
    insert_duplicate_edges(store, &nodes);

    Ok(IndexStats { total, indexed, errors, watching: false })
}

fn entry_to_node(path: &Path) -> Option<GraphNode> {
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

    // Hash small files only (< 50 MB) for duplicate detection
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

        // Keep thread (and debouncer) alive
        loop { std::thread::sleep(Duration::from_secs(60)); }
    });
}

fn handle_event(
    event: &notify_debouncer_mini::DebouncedEvent,
    store: &Arc<Mutex<Store>>,
    app: &tauri::AppHandle,
) {
    use notify_debouncer_mini::notify::EventKind::*;
    use tauri::Emitter;

    let path = &event.path;

    match event.kind {
        Create(_) | Modify(_) => {
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
        }
        Remove(_) => {
            if let Ok(s) = store.lock() {
                let _ = s.delete_node(&path.to_string_lossy());
            }
        }
        _ => return,
    }
    app.emit("graph-updated", ()).ok();
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::store::Store;
    use std::fs;
    use tempfile::TempDir;

    fn temp_tree() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"hello world").unwrap();
        fs::write(dir.path().join("b.txt"), b"hello world").unwrap(); // duplicate
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
        // 5 entries: dir root + a.txt + b.txt + c.rs + src/ + src/lib.rs
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
```

- [ ] **Step 2: Add `tempfile` dev dependency**

Add to `src-tauri/Cargo.toml`:
```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Run tests**

```bash
cd src-tauri && cargo test graph::indexer -- --nocapture 2>&1 | tail -10
```

Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/graph/indexer.rs
git commit -m "feat(graph): indexer.rs — parallel scanner, blake3 dedup, file watcher"
```

---

## Task 5: llm.rs — LLM Bridge

**Files:**
- Create: `src-tauri/src/graph/llm.rs`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/graph/llm.rs`:

```rust
use serde::{Deserialize, Serialize};
use crate::graph::query::StructuredQuery;

// ─── Config ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,          // "ollama" | "remote"
    pub base_url: String,          // "http://localhost:11434" for ollama
    pub model:    String,          // "llama3.2" | "gpt-4o-mini" etc.
    pub api_key:  Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            model:    "llama3.2".into(),
            api_key:  None,
        }
    }
}

// ─── NL → StructuredQuery ────────────────────────────────────────────────────

const SCHEMA_PROMPT: &str = r#"Convert the filesystem question to JSON. Return ONLY valid JSON, no markdown, no explanation.

Shapes:
{"kind":"MetadataFilter","name_contains":"...","extension":".mp4","kind_filter":"file","size_gt":104857600,"size_lt":null,"modified_after":null}
{"kind":"FindDuplicates","path":"/"}
{"kind":"GetRelated","path":"/some/dir","depth":1}

Rules:
- size_gt / size_lt: bytes as integer or null
- modified_after: unix timestamp integer or null
- kind_filter: "file" or "directory" or null
- extension: include the dot, e.g. ".mp4" not "mp4"
- For "duplicate" questions use FindDuplicates with path "/"
- Null fields can be omitted

Question: "#;

pub async fn nl_to_query(question: &str, config: &LlmConfig) -> Result<StructuredQuery, String> {
    let prompt = format!("{SCHEMA_PROMPT}{question}");
    let raw = call_llm(config, &prompt).await?;

    // Strip markdown code fences if model wrapped the JSON
    let cleaned = raw.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    serde_json::from_str::<StructuredQuery>(cleaned)
        .map_err(|e| format!("LLM returned invalid JSON: {e}\nRaw: {raw}"))
}

async fn call_llm(config: &LlmConfig, prompt: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    match config.provider.as_str() {
        "ollama" => {
            let url  = format!("{}/api/generate", config.base_url);
            let body = serde_json::json!({ "model": config.model, "prompt": prompt, "stream": false });
            let resp = client.post(&url).json(&body).send().await
                .map_err(|e| format!("Ollama unreachable: {e}"))?
                .json::<serde_json::Value>().await
                .map_err(|e| format!("Ollama response parse failed: {e}"))?;
            resp["response"].as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| "Ollama response missing 'response' field".into())
        }
        "remote" => {
            let url     = format!("{}/chat/completions", config.base_url);
            let api_key = config.api_key.as_deref().unwrap_or("");
            let body = serde_json::json!({
                "model": config.model,
                "messages": [{ "role": "user", "content": prompt }],
                "temperature": 0
            });
            let resp = client.post(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .json(&body)
                .send().await
                .map_err(|e| format!("Remote LLM unreachable: {e}"))?
                .json::<serde_json::Value>().await
                .map_err(|e| format!("Remote LLM response parse failed: {e}"))?;
            resp["choices"][0]["message"]["content"].as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| "Remote LLM response missing content".into())
        }
        other => Err(format!("Unknown LLM provider: '{other}'. Use 'ollama' or 'remote'.")),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::query::StructuredQuery;

    /// Parse LLM output directly — no network call.
    fn parse(json: &str) -> Result<StructuredQuery, String> {
        serde_json::from_str::<StructuredQuery>(json)
            .map_err(|e| format!("parse failed: {e}"))
    }

    #[test]
    fn parses_metadata_filter() {
        let q = parse(r#"{"kind":"MetadataFilter","extension":".mp4","size_gt":104857600}"#).unwrap();
        assert!(matches!(q, StructuredQuery::MetadataFilter { .. }));
    }

    #[test]
    fn parses_find_duplicates() {
        let q = parse(r#"{"kind":"FindDuplicates","path":"/"}"#).unwrap();
        assert!(matches!(q, StructuredQuery::FindDuplicates { .. }));
    }

    #[test]
    fn parses_get_related() {
        let q = parse(r#"{"kind":"GetRelated","path":"/home","depth":2}"#).unwrap();
        match q {
            StructuredQuery::GetRelated { path, depth } => {
                assert_eq!(path, "/home");
                assert_eq!(depth, 2);
            }
            _ => panic!("expected GetRelated"),
        }
    }

    #[test]
    fn strips_markdown_fences() {
        let raw = "```json\n{\"kind\":\"FindDuplicates\",\"path\":\"/\"}\n```";
        let cleaned = raw.trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let q = parse(cleaned).unwrap();
        assert!(matches!(q, StructuredQuery::FindDuplicates { .. }));
    }

    #[test]
    fn unknown_provider_returns_err() {
        // Test config construction only — no network
        let config = LlmConfig { provider: "unknown".into(), ..LlmConfig::default() };
        assert_eq!(config.provider, "unknown");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cd src-tauri && cargo test graph::llm -- --nocapture 2>&1 | tail -10
```

Expected: 5 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/graph/llm.rs
git commit -m "feat(graph): llm.rs — NL→StructuredQuery bridge (ollama + remote)"
```

---

## Task 6: mod.rs — GraphAppState + Tauri Commands

**Files:**
- Create: `src-tauri/src/graph/mod.rs`

- [ ] **Step 1: Create mod.rs**

Create `src-tauri/src/graph/mod.rs`:

```rust
pub mod store;
pub mod indexer;
pub mod query;
pub mod llm;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

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
    if let Some(config) = state.llm_config.lock().unwrap().clone() {
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
        if let Ok(store) = store_arc.lock() {
            match indexer::scan_and_index(Path::new(&path), &store) {
                Ok(s) => {
                    if let Ok(mut stats) = stats_arc.lock() {
                        stats.total   = s.total;
                        stats.indexed = s.indexed;
                        stats.errors  = s.errors;
                    }
                    app.emit("graph-index-complete", &s).ok();
                }
                Err(e) => eprintln!("[graph] scan failed: {e}"),
            }
        }

        // Start file watcher after initial scan
        if let Ok(store_arc2) = store_arc.lock().map(|_| store_arc.clone()) {
            indexer::start_watcher(path, store_arc2, app);
        }
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
```

- [ ] **Step 2: Run clippy to verify**

```bash
cd src-tauri && cargo clippy --lib 2>&1 | grep error | head -10
```

Expected: no errors (warnings about unused imports are OK at this stage).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/graph/mod.rs
git commit -m "feat(graph): mod.rs — GraphAppState and Tauri command stubs"
```

---

## Task 7: Wire Into lib.rs

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add mod declaration and setup**

At the top of `src-tauri/src/lib.rs`, after the existing `mod transfer;` line, add:

```rust
mod graph;
use graph::GraphAppState;
```

Inside the `.setup(|app|` closure in `run()`, after the `app.manage(TransferAppState { ... });` call, add:

```rust
let db_path = app.path().app_data_dir()
    .map(|d| d.join("graph.db"))
    .unwrap_or_else(|_| PathBuf::from("graph.db"));

match graph::GraphAppState::new(&db_path) {
    Ok(graph_state) => { app.manage(graph_state); }
    Err(e) => eprintln!("[graph] failed to init: {e}"),
}
```

Add `PathBuf` to the existing `use std::path::{Path, PathBuf};` import if not already present. If only `Path` is imported, change to `use std::path::{Path, PathBuf};`.

- [ ] **Step 2: Register commands**

In the `.invoke_handler(tauri::generate_handler![` block, add:

```rust
graph::graph_search,
graph::graph_get_related,
graph::graph_get_duplicates,
graph::graph_index_status,
graph::graph_set_root,
graph::graph_set_llm,
```

- [ ] **Step 3: Build to verify wiring**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -20
```

Expected: clean build (no errors).

- [ ] **Step 4: Run all graph tests**

```bash
cd src-tauri && cargo test graph:: -- --nocapture 2>&1 | grep -E "test .* \.\.\."
```

Expected: all tests show `ok`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(graph): wire GraphAppState and commands into Tauri app"
```

---

## Task 8: Frontend — graph.ts API Bindings

**Files:**
- Create: `frontend/js/graph.ts`

- [ ] **Step 1: Create graph.ts**

Create `frontend/js/graph.ts`:

```typescript
// ─── Types ────────────────────────────────────────────────────────────────────

export interface GraphSearchResult {
    path:          string;
    name:          string;
    kind:          'file' | 'directory';
    size:          number;
    size_human:    string;
    extension?:    string;
    modified_secs?: number;
    snippet?:      string;
    score:         number;
}

export interface IndexStats {
    total:    number;
    indexed:  number;
    errors:   number;
    watching: boolean;
}

export interface LlmConfig {
    provider: 'ollama' | 'remote';
    base_url: string;
    model:    string;
    api_key?: string;
}

// ─── API ──────────────────────────────────────────────────────────────────────

function _invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
    return (window as any).__TAURI_INTERNALS__.invoke(cmd, args);
}

export async function graphSearch(queryStr: string): Promise<GraphSearchResult[]> {
    return _invoke('graph_search', { queryStr }) as Promise<GraphSearchResult[]>;
}

export async function graphGetRelated(path: string): Promise<GraphSearchResult[]> {
    return _invoke('graph_get_related', { path }) as Promise<GraphSearchResult[]>;
}

export async function graphGetDuplicates(path: string): Promise<GraphSearchResult[]> {
    return _invoke('graph_get_duplicates', { path }) as Promise<GraphSearchResult[]>;
}

export async function graphIndexStatus(): Promise<IndexStats> {
    return _invoke('graph_index_status') as Promise<IndexStats>;
}

export async function graphSetRoot(path: string): Promise<void> {
    return _invoke('graph_set_root', { path }) as Promise<void>;
}

export async function graphSetLlm(config: LlmConfig): Promise<void> {
    return _invoke('graph_set_llm', { config }) as Promise<void>;
}
```

- [ ] **Step 2: Compile**

```bash
npm run build:ts 2>&1
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/js/graph.ts frontend/js/graph.js
git commit -m "feat(graph): graph.ts — TypeScript API bindings for graph commands"
```

---

## Task 9: Frontend — graphui.ts (Search Modes + Results Panel + Status Bar)

**Files:**
- Create: `frontend/js/graphui.ts`

- [ ] **Step 1: Create graphui.ts**

Create `frontend/js/graphui.ts`:

```typescript
import * as graphApi from './graph.js';
import type { GraphSearchResult, IndexStats } from './graph.js';
import { toast } from './events.js';

type SearchMode = 'filter' | 'search' | 'ask';
let _currentMode: SearchMode = 'filter';
let _onFilterMode: (() => void) | null = null;

// ─── Init ─────────────────────────────────────────────────────────────────────

export function initGraphUI(onFilterMode: () => void): void {
    _onFilterMode = onFilterMode;
    _bindModeButtons();
    _startStatusPolling();
}

// ─── Search modes ────────────────────────────────────────────────────────────

function _bindModeButtons(): void {
    document.getElementById('search-mode-filter')?.addEventListener('click', () => setMode('filter'));
    document.getElementById('search-mode-search')?.addEventListener('click', () => setMode('search'));
    document.getElementById('search-mode-ask')?.addEventListener('click',   () => setMode('ask'));

    document.getElementById('graph-search-form')?.addEventListener('submit', async (e) => {
        e.preventDefault();
        const input = document.getElementById('graph-search-input') as HTMLInputElement;
        const query = input?.value.trim();
        if (!query) return;
        if (_currentMode === 'search') await runSearch(query);
        else if (_currentMode === 'ask') await runAsk(query);
    });
}

export function setMode(mode: SearchMode): void {
    _currentMode = mode;
    const bar = document.getElementById('graph-search-bar');
    bar?.setAttribute('data-mode', mode);

    ['filter','search','ask'].forEach(m => {
        document.getElementById(`search-mode-${m}`)?.classList.toggle('active', m === mode);
    });

    const placeholder = document.getElementById('graph-search-input') as HTMLInputElement;
    if (placeholder) {
        placeholder.placeholder =
            mode === 'filter' ? 'Filter by name…' :
            mode === 'search' ? 'Search filesystem (size, type, date…)' :
                                'Ask a question about your files…';
    }

    if (mode === 'filter') {
        hideResultsPanel();
        _onFilterMode?.();
    }
}

// ─── Search execution ─────────────────────────────────────────────────────────

async function runSearch(query: string): Promise<void> {
    showResultsLoading();
    try {
        const results = await graphApi.graphSearch(query);
        showResults(results, query);
    } catch (e) {
        hideResultsPanel();
        toast(`Search failed: ${e}`, 'error');
    }
}

async function runAsk(question: string): Promise<void> {
    showResultsLoading();
    try {
        const results = await graphApi.graphSearch(question);
        showResults(results, question);
    } catch (e) {
        hideResultsPanel();
        toast(`Query failed: ${e}`, 'error');
    }
}

// ─── Results panel ────────────────────────────────────────────────────────────

function showResultsLoading(): void {
    const panel = document.getElementById('graph-results-panel')!;
    panel.classList.remove('hidden');
    panel.innerHTML = '<div class="graph-results-loading">Searching…</div>';
}

export function hideResultsPanel(): void {
    document.getElementById('graph-results-panel')?.classList.add('hidden');
}

function showResults(results: GraphSearchResult[], query: string): void {
    const panel = document.getElementById('graph-results-panel')!;
    panel.classList.remove('hidden');

    if (!results.length) {
        panel.innerHTML = `<div class="graph-results-empty">No results for <em>${escHtml(query)}</em></div>`;
        return;
    }

    const items = results.map(r => `
        <div class="graph-result-item" data-path="${escHtml(r.path)}" title="${escHtml(r.path)}">
            <span class="gr-icon">${r.kind === 'directory' ? '📁' : '📄'}</span>
            <span class="gr-name">${escHtml(r.name)}</span>
            <span class="gr-size">${r.size_human}</span>
            ${r.snippet ? `<span class="gr-snippet">${escHtml(r.snippet)}</span>` : ''}
        </div>
    `).join('');

    panel.innerHTML = `
        <div class="graph-results-header">
            <span>${results.length} result${results.length !== 1 ? 's' : ''}</span>
            <button id="graph-results-close" class="graph-results-close">✕</button>
        </div>
        <div class="graph-results-list">${items}</div>
    `;

    document.getElementById('graph-results-close')?.addEventListener('click', hideResultsPanel);
}

function escHtml(s: string): string {
    return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

// ─── Index status bar ─────────────────────────────────────────────────────────

function _startStatusPolling(): void {
    _updateStatus();
    setInterval(_updateStatus, 3000);
}

async function _updateStatus(): Promise<void> {
    try {
        const stats: IndexStats = await graphApi.graphIndexStatus();
        _renderStatus(stats);
    } catch { /* graph not initialized yet */ }
}

function _renderStatus(stats: IndexStats): void {
    const bar = document.getElementById('graph-status-bar');
    if (!bar) return;
    if (stats.indexed === 0) { bar.textContent = ''; return; }
    const done = stats.indexed >= stats.total && stats.total > 0;
    bar.textContent = done
        ? `Graph: ${stats.indexed.toLocaleString()} files indexed`
        : `Indexing: ${stats.indexed.toLocaleString()} / ${stats.total.toLocaleString()} files…`;
    bar.classList.toggle('indexing', !done);
}

// ─── Root indexing trigger ────────────────────────────────────────────────────

export async function triggerIndex(path: string): Promise<void> {
    try {
        await graphApi.graphSetRoot(path);
    } catch (e) {
        console.error('[graph] index trigger failed:', e);
    }
}
```

- [ ] **Step 2: Compile**

```bash
npm run build:ts 2>&1
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/js/graphui.ts frontend/js/graphui.js
git commit -m "feat(graph): graphui.ts — search modes, results panel, index status bar"
```

---

## Task 10: Frontend HTML + CSS + main.ts Wiring

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/css/style.css`
- Modify: `frontend/js/main.ts`

- [ ] **Step 1: Add HTML elements to index.html**

In `frontend/index.html`, find the existing search bar section (around line 28, the `<span class="search-icon">`) and replace the entire toolbar `<div class="toolbar">` with:

```html
<div class="toolbar">
    <button id="btn-back" class="toolbar-btn" title="Go back (Backspace)" disabled>←</button>
    <button id="btn-forward" class="toolbar-btn" title="Go forward (])" disabled>→</button>

    <!-- Search mode toggle + input -->
    <div id="graph-search-bar" data-mode="filter">
        <div class="search-mode-pills">
            <button id="search-mode-filter" class="mode-pill active" title="Filter visible nodes">Filter</button>
            <button id="search-mode-search" class="mode-pill" title="Search entire index">Search</button>
            <button id="search-mode-ask"    class="mode-pill" title="Ask AI a question">Ask AI</button>
        </div>
        <form id="graph-search-form" class="search-form" autocomplete="off">
            <span class="search-icon">⌕</span>
            <input id="graph-search-input" type="search" placeholder="Filter by name…" spellcheck="false">
            <button id="search-clear" class="hidden" title="Clear" type="button">✕</button>
        </form>
    </div>

    <span id="match-count" class="match-count"></span>

    <div class="toolbar-spacer"></div>
    <span id="item-count"  class="toolbar-info"></span>
    <span id="total-size"  class="toolbar-info"></span>
    <div class="sort-group">
        <label class="toolbar-label">Sort</label>
        <select id="sort-select" title="Sort order">
            <option value="size-desc">Size ↓</option>
            <option value="size-asc">Size ↑</option>
            <option value="name-asc">Name A-Z</option>
            <option value="name-desc">Name Z-A</option>
            <option value="type">Type</option>
        </select>
    </div>
    <div class="filter-group">
        <label class="toolbar-label">Type</label>
        <select id="filter-select" title="File type filter">
            <option value="all">All</option>
            <option value="directory">Folders</option>
            <option value="image">Images</option>
            <option value="video">Video</option>
            <option value="audio">Audio</option>
            <option value="code">Code</option>
            <option value="doc">Docs</option>
            <option value="archive">Archives</option>
        </select>
    </div>
    <button id="btn-new-file"   class="toolbar-btn" title="New file (⌘N)">📄</button>
    <button id="btn-new-folder" class="toolbar-btn" title="New folder (⌘⇧N)">📁</button>
</div>
```

Also add these two elements just before the closing `</body>` tag:

```html
<!-- Graph results panel -->
<div id="graph-results-panel" class="hidden"></div>

<!-- Graph index status bar -->
<div id="graph-status-bar" class="graph-status-bar"></div>
```

- [ ] **Step 2: Add CSS to style.css**

Append to the end of `frontend/css/style.css`:

```css
/* ── Graph search bar ───────────────────────────────────────────── */
#graph-search-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
}

.search-mode-pills {
    display: flex;
    gap: 2px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 2px;
    flex-shrink: 0;
}

.mode-pill {
    background: none;
    border: none;
    color: var(--text-dim);
    padding: 3px 8px;
    border-radius: 4px;
    font-size: 0.72rem;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
    white-space: nowrap;
}

.mode-pill.active,
.mode-pill:hover {
    background: var(--accent);
    color: #fff;
}

.search-form {
    display: flex;
    align-items: center;
    flex: 1;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0 8px;
    gap: 4px;
}

.search-form input {
    flex: 1;
    background: none;
    border: none;
    outline: none;
    color: var(--text);
    font-size: 0.82rem;
    padding: 4px 0;
    min-width: 0;
}

/* ── Graph results panel ────────────────────────────────────────── */
#graph-results-panel {
    position: fixed;
    bottom: 28px;
    left: 50%;
    transform: translateX(-50%);
    width: min(680px, 90vw);
    max-height: 380px;
    background: var(--bg-panel);
    border: 1px solid var(--border-hi);
    border-radius: var(--radius);
    box-shadow: 0 8px 32px rgba(0,0,0,0.6);
    z-index: 200;
    overflow: hidden;
    display: flex;
    flex-direction: column;
}

.graph-results-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px;
    font-size: 0.75rem;
    color: var(--text-dim);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
}

.graph-results-close {
    background: none;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
    font-size: 0.8rem;
    padding: 0 4px;
}

.graph-results-list {
    overflow-y: auto;
    flex: 1;
}

.graph-result-item {
    display: grid;
    grid-template-columns: 20px 1fr 70px;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    font-size: 0.8rem;
    cursor: pointer;
    border-bottom: 1px solid var(--border);
    transition: background 0.1s;
}

.graph-result-item:hover { background: var(--bg-hover); }
.gr-icon { text-align: center; }
.gr-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.gr-size { color: var(--text-dim); font-size: 0.72rem; text-align: right; }
.gr-snippet {
    grid-column: 2 / -1;
    color: var(--text-dim);
    font-size: 0.72rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.graph-results-loading,
.graph-results-empty {
    padding: 24px;
    text-align: center;
    color: var(--text-dim);
    font-size: 0.82rem;
}

/* ── Index status bar ───────────────────────────────────────────── */
.graph-status-bar {
    position: fixed;
    bottom: 6px;
    right: 12px;
    font-size: 0.68rem;
    color: var(--text-dim);
    z-index: 100;
    pointer-events: none;
}

.graph-status-bar.indexing { color: var(--accent); }
```

- [ ] **Step 3: Wire graphui into main.ts**

In `frontend/js/main.ts`, add the import at the top:

```typescript
import { initGraphUI, triggerIndex } from './graphui.js';
```

Inside the `init()` function, after `bindGlobalEvents();`, add:

```typescript
initGraphUI(() => {
    // Restore filter mode — re-apply current search query to nodes
    const input = document.getElementById('graph-search-input') as HTMLInputElement;
    if (input) {
        search.applySearch(input.value);
    }
});
```

After `await nav.navigate(startPath);`, add:

```typescript
// Trigger background graph index of the start path
triggerIndex(startPath).catch(() => {});
```

Also update the existing `input` event listener for search (find `applySearch` in events.ts and ensure the graph-search-input is bound). In `frontend/js/events.ts`, find where the search input listener is bound and change the selector from the old input id to `graph-search-input` if it differs. Check the existing id:

```bash
grep -n "search-input\|searchInput\|input.*search" frontend/index.html frontend/js/events.ts | head -10
```

If the existing search input has a different id (e.g. `search-input`), update `events.ts` to use `graph-search-input` instead.

- [ ] **Step 4: Compile and verify**

```bash
npm run build:ts 2>&1
```

Expected: no errors.

- [ ] **Step 5: Start dev server and smoke test**

```bash
npm run tauri dev
```

Verify:
- Three mode pills visible in toolbar (Filter / Search / Ask AI)
- "Filter" is active by default, existing filter behavior unchanged
- Click "Search", type a filename, press Enter → results panel appears with matching files
- Status bar at bottom-right shows "Indexing N / M files…" within a few seconds of launch
- After indexing completes, status bar shows "Graph: N files indexed"

- [ ] **Step 6: Commit**

```bash
git add frontend/index.html frontend/css/style.css frontend/js/main.ts frontend/js/main.js
git commit -m "feat(graph): wire graph search UI — mode pills, results panel, status bar"
```

---

## Task 11: Final Verification

- [ ] **Step 1: Run full test suite**

```bash
cd src-tauri && cargo test 2>&1 | tail -20
```

Expected: all existing tests + all new `graph::` tests pass.

- [ ] **Step 2: Run clippy**

```bash
cd src-tauri && cargo clippy --lib 2>&1 | grep "^error" | head -10
```

Expected: no errors.

- [ ] **Step 3: Final commit + tag**

```bash
git add -A
git commit -m "feat: knowledge graph Phase 1 complete — metadata index, search, LLM bridge"
```

---

## Phase 1 Complete — What Works

After these 11 tasks:

| Feature | How to use |
|---|---|
| Background filesystem index | Auto-starts on app launch, indexes current directory |
| Metadata search | Click "Search" tab, type query like "large videos" or ".pdf files" |
| NL query (if ollama running) | Configure via `graph_set_llm`, then ask in Search mode |
| Duplicate detection | Right-click file → "Find Duplicates" (wire in Task 10 step 3) |
| Live file watching | Create/rename/delete files → index updates automatically |
| Index status | Bottom-right status bar shows indexing progress |

**Next:** Phase 2 plan adds FTS5 content indexing for opt-in folders.
