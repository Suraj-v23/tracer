# Knowledge Graph — Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add opt-in full-text search over file contents using SQLite FTS5, surfacing BM25-ranked results with matched snippets, and expose Tauri commands + frontend UI to manage indexed folders.

**Architecture:** New `content.rs` module handles text extraction (read file → strip binary → store in FTS5 virtual table). `store.rs` gets the FTS5 schema and content search query. `mod.rs` gains 4 new commands. `graph_search` falls through to content search when no metadata results match. Frontend adds a right-click "Deep Index This Folder" menu item and a settings panel listing indexed folders.

**Tech Stack:** SQLite FTS5 (already in `rusqlite bundled-full`), `rayon` (existing), TypeScript (existing `tsc` pipeline).

---

## File Map

**Create (Rust):**
- `src-tauri/src/graph/content.rs` — text extractor + FTS indexer

**Modify (Rust):**
- `src-tauri/src/graph/store.rs` — add FTS5 table to schema, add `index_content`, `content_search`, `indexed_folders` CRUD
- `src-tauri/src/graph/mod.rs` — add `pub mod content`, 4 new Tauri commands, upgrade `graph_search` to merge content results
- `src-tauri/src/graph/query.rs` — add `ContentSearch` variant to `StructuredQuery`, update `heuristic_parse`

**Modify (Frontend):**
- `frontend/js/graph.ts` — add 4 new API bindings + `ContentSearchResult` type
- `frontend/js/graphui.ts` — add indexed folders settings panel, upgrade results to show snippets
- `frontend/index.html` — add "Deep Index" context menu item, settings panel HTML
- `frontend/css/style.css` — styles for snippet display and settings panel

---

## Task 1: FTS5 Schema + Store Methods

**Files:**
- Modify: `src-tauri/src/graph/store.rs`

- [ ] **Step 1: Write failing tests first**

Add to the `#[cfg(test)]` block in `src-tauri/src/graph/store.rs`:

```rust
#[test]
fn fts_index_and_search() {
    let store = Store::open_in_memory().unwrap();
    // Insert a node first (FTS has a foreign-key-like dependency on nodes.id)
    store.upsert_node(&make_node("/a/readme.md", "readme.md", "file", 100)).unwrap();
    let id = store.get_node_id("/a/readme.md").unwrap().unwrap();
    store.index_content(id, "# Hello World\nThis file talks about authentication tokens.").unwrap();
    let results = store.content_search("authentication").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "readme.md");
    assert!(results[0].snippet.is_some());
}

#[test]
fn fts_returns_empty_for_no_match() {
    let store = Store::open_in_memory().unwrap();
    store.upsert_node(&make_node("/a/readme.md", "readme.md", "file", 100)).unwrap();
    let id = store.get_node_id("/a/readme.md").unwrap().unwrap();
    store.index_content(id, "hello world").unwrap();
    let results = store.content_search("zxqwerty").unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn indexed_folders_crud() {
    let store = Store::open_in_memory().unwrap();
    store.add_indexed_folder("/home/projects").unwrap();
    let folders = store.list_indexed_folders().unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0], "/home/projects");
    store.remove_indexed_folder("/home/projects").unwrap();
    let folders = store.list_indexed_folders().unwrap();
    assert_eq!(folders.len(), 0);
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cd src-tauri && cargo test graph::store::tests::fts -- --nocapture 2>&1 | tail -10
```

Expected: compile error (methods don't exist yet).

- [ ] **Step 3: Add FTS5 table to `migrate()` in store.rs**

In the `migrate()` method, inside the `execute_batch` string after the `settings` table definition, add:

```rust
            CREATE VIRTUAL TABLE IF NOT EXISTS fts_content USING fts5(
                content,
                content='',
                tokenize='porter ascii'
            );

            CREATE TABLE IF NOT EXISTS fts_node_map (
                rowid   INTEGER PRIMARY KEY,
                node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE
            );
```

Note: we use a separate `fts_node_map` table to link FTS rowids to node ids, since FTS5 rowids are internal.

- [ ] **Step 4: Add content methods to `Store` impl in store.rs**

After the `set_setting` method, add:

```rust
// ── FTS Content Indexing ──────────────────────────────────────────────────

pub fn index_content(&self, node_id: i64, text: &str) -> SqlResult<()> {
    // Remove old entry if exists
    let old_rowid: Option<i64> = {
        let mut s = self.conn.prepare_cached(
            "SELECT rowid FROM fts_node_map WHERE node_id=?1"
        )?;
        let mut rows = s.query(params![node_id])?;
        rows.next()?.map(|r| r.get(0)).transpose()?
    };
    if let Some(rowid) = old_rowid {
        self.conn.execute("DELETE FROM fts_content WHERE rowid=?1", params![rowid])?;
        self.conn.execute("DELETE FROM fts_node_map WHERE node_id=?1", params![node_id])?;
    }

    // Insert new FTS entry
    self.conn.execute(
        "INSERT INTO fts_content(content) VALUES (?1)",
        params![text],
    )?;
    let rowid = self.conn.last_insert_rowid();
    self.conn.execute(
        "INSERT INTO fts_node_map(rowid, node_id) VALUES (?1, ?2)",
        params![rowid, node_id],
    )?;
    Ok(())
}

pub fn content_search(&self, query: &str) -> SqlResult<Vec<SearchResult>> {
    let mut stmt = self.conn.prepare(r#"
        SELECT n.path, n.name, n.kind, n.size, n.extension, n.modified_secs,
               snippet(fts_content, 0, '[', ']', '...', 16) AS snip
        FROM fts_content f
        JOIN fts_node_map m ON m.rowid = f.rowid
        JOIN nodes n ON n.id = m.node_id
        WHERE fts_content MATCH ?1
        ORDER BY rank
        LIMIT 100
    "#)?;
    let results = stmt.query_map(params![query], |row| {
        let size: i64 = row.get(3)?;
        Ok(SearchResult {
            path:          row.get(0)?,
            name:          row.get(1)?,
            kind:          row.get(2)?,
            size:          size as u64,
            size_human:    format_size(size as u64),
            extension:     row.get(4)?,
            modified_secs: row.get(5)?,
            snippet:       row.get(6)?,
            score:         1.0,
        })
    })?
    .filter_map(|r| r.ok())
    .collect();
    Ok(results)
}

// ── Indexed Folders ───────────────────────────────────────────────────────

pub fn add_indexed_folder(&self, path: &str) -> SqlResult<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    self.conn.execute(
        "INSERT OR REPLACE INTO indexed_folders(path, added_secs) VALUES (?1, ?2)",
        params![path, now],
    )?;
    Ok(())
}

pub fn remove_indexed_folder(&self, path: &str) -> SqlResult<()> {
    self.conn.execute(
        "DELETE FROM indexed_folders WHERE path=?1",
        params![path],
    )?;
    Ok(())
}

pub fn list_indexed_folders(&self) -> SqlResult<Vec<String>> {
    let mut stmt = self.conn.prepare("SELECT path FROM indexed_folders ORDER BY added_secs")?;
    let results = stmt.query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(results)
}

pub fn is_folder_indexed(&self, path: &str) -> SqlResult<bool> {
    let mut stmt = self.conn.prepare_cached(
        "SELECT 1 FROM indexed_folders WHERE ?1 LIKE path || '%' LIMIT 1"
    )?;
    let mut rows = stmt.query(params![path])?;
    Ok(rows.next()?.is_some())
}
```

- [ ] **Step 5: Run tests**

```bash
cd src-tauri && cargo test graph::store -- --nocapture 2>&1 | tail -15
```

Expected: all store tests pass including the 3 new ones (10 total).

- [ ] **Step 6: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/store.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p2): FTS5 schema, index_content, content_search, indexed_folders CRUD"
```

---

## Task 2: content.rs — Text Extractor

**Files:**
- Create: `src-tauri/src/graph/content.rs`

- [ ] **Step 1: Write failing tests first**

Create `src-tauri/src/graph/content.rs` with just the test module to start:

```rust
// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_from_ts_file() {
        let text = extract_text_from_bytes(b"export function hello() { return 42; }", ".ts");
        assert!(text.is_some());
        let t = text.unwrap();
        assert!(t.contains("hello"));
    }

    #[test]
    fn skips_binary_content() {
        // PNG magic bytes
        let binary = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
        let text = extract_text_from_bytes(binary, ".png");
        assert!(text.is_none());
    }

    #[test]
    fn skips_unsupported_extension() {
        let text = extract_text_from_bytes(b"some content", ".exe");
        assert!(text.is_none());
    }

    #[test]
    fn respects_size_limit() {
        // 3MB of 'a' — over the 2MB limit
        let big = vec![b'a'; 3 * 1024 * 1024];
        let text = extract_text_from_bytes(&big, ".txt");
        assert!(text.is_none());
    }

    #[test]
    fn indexes_folder_scans_files() {
        use crate::graph::store::Store;
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hello.ts"), b"export function greet() {}").unwrap();
        fs::write(dir.path().join("notes.md"), b"# Notes\nThis is about authentication").unwrap();
        fs::write(dir.path().join("binary.png"), b"\x89PNG\r\n\x1a\n").unwrap();

        let store = Store::open_in_memory().unwrap();
        // Seed nodes so index_content can link them
        store.upsert_node(&crate::graph::store::GraphNode {
            id: 0, path: dir.path().join("hello.ts").to_string_lossy().to_string(),
            name: "hello.ts".into(), kind: "file".into(), size: 26,
            extension: Some(".ts".into()), modified_secs: None, created_secs: None, content_hash: None,
        }).unwrap();
        store.upsert_node(&crate::graph::store::GraphNode {
            id: 0, path: dir.path().join("notes.md").to_string_lossy().to_string(),
            name: "notes.md".into(), kind: "file".into(), size: 37,
            extension: Some(".md".into()), modified_secs: None, created_secs: None, content_hash: None,
        }).unwrap();

        let count = index_folder(dir.path(), &store);
        assert_eq!(count, 2, "should index 2 text files, skip binary");

        let results = store.content_search("authentication").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "notes.md");
    }
}
```

- [ ] **Step 2: Implement content.rs**

Replace the file content with the full implementation:

```rust
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::graph::store::Store;

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024; // 2 MB

const TEXT_EXTENSIONS: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx", ".mjs",
    ".py", ".rs", ".go", ".java", ".c", ".cpp", ".h", ".hpp",
    ".md", ".txt", ".rst",
    ".json", ".yaml", ".yml", ".toml", ".xml",
    ".html", ".htm", ".css", ".scss", ".sass",
    ".sh", ".bash", ".zsh", ".fish",
    ".sql", ".graphql", ".proto",
    ".env", ".gitignore", ".dockerfile",
];

/// Returns extracted text if the file is indexable, None otherwise.
pub fn extract_text_from_bytes(bytes: &[u8], extension: &str) -> Option<String> {
    let ext = extension.to_lowercase();

    // Must be a known text extension
    if !TEXT_EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }

    // Size check (caller should check file size before reading, but guard here too)
    if bytes.len() > MAX_FILE_BYTES as usize {
        return None;
    }

    // Heuristic binary detection: if >5% non-UTF8 bytes in first 512 bytes, skip
    let sample = &bytes[..bytes.len().min(512)];
    let non_utf8 = sample.iter().filter(|&&b| b < 9 || (b > 13 && b < 32 && b != 27)).count();
    if non_utf8 > sample.len() / 20 {
        return None;
    }

    String::from_utf8(bytes.to_vec())
        .ok()
        .map(|s| s.chars().take(50_000).collect()) // cap at 50k chars per file
}

/// Index all text files under `root` that are already present in the node store.
/// Returns count of files successfully indexed.
pub fn index_folder(root: &Path, store: &Store) -> usize {
    let entries: Vec<_> = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.metadata().map(|m| m.len() <= MAX_FILE_BYTES).unwrap_or(false)
        })
        .collect();

    let count = Arc::new(AtomicUsize::new(0));

    // Parallel text extraction — pure CPU/IO, no store access yet
    let extracted: Vec<(String, String)> = entries.par_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let ext = path.extension()
                .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                .unwrap_or_default();
            let bytes = std::fs::read(path).ok()?;
            let text = extract_text_from_bytes(&bytes, &ext)?;
            Some((path.to_string_lossy().to_string(), text))
        })
        .collect();

    // Sequential store writes (SQLite single-writer)
    let _ = store.conn.execute("BEGIN", []);
    for (path, text) in &extracted {
        if let Ok(Some(node_id)) = store.get_node_id(path) {
            if store.index_content(node_id, text).is_ok() {
                count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    let _ = store.conn.execute("COMMIT", []);

    count.load(Ordering::Relaxed)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_from_ts_file() {
        let text = extract_text_from_bytes(b"export function hello() { return 42; }", ".ts");
        assert!(text.is_some());
        let t = text.unwrap();
        assert!(t.contains("hello"));
    }

    #[test]
    fn skips_binary_content() {
        let binary = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
        let text = extract_text_from_bytes(binary, ".png");
        assert!(text.is_none());
    }

    #[test]
    fn skips_unsupported_extension() {
        let text = extract_text_from_bytes(b"some content", ".exe");
        assert!(text.is_none());
    }

    #[test]
    fn respects_size_limit() {
        let big = vec![b'a'; 3 * 1024 * 1024];
        let text = extract_text_from_bytes(&big, ".txt");
        assert!(text.is_none());
    }

    #[test]
    fn indexes_folder_scans_files() {
        use crate::graph::store::{GraphNode, Store};
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hello.ts"), b"export function greet() {}").unwrap();
        fs::write(dir.path().join("notes.md"), b"# Notes\nThis is about authentication").unwrap();
        fs::write(dir.path().join("binary.png"), b"\x89PNG\r\n\x1a\n").unwrap();

        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&GraphNode {
            id: 0, path: dir.path().join("hello.ts").to_string_lossy().to_string(),
            name: "hello.ts".into(), kind: "file".into(), size: 26,
            extension: Some(".ts".into()), modified_secs: None, created_secs: None, content_hash: None,
        }).unwrap();
        store.upsert_node(&GraphNode {
            id: 0, path: dir.path().join("notes.md").to_string_lossy().to_string(),
            name: "notes.md".into(), kind: "file".into(), size: 37,
            extension: Some(".md".into()), modified_secs: None, created_secs: None, content_hash: None,
        }).unwrap();

        let count = index_folder(dir.path(), &store);
        assert_eq!(count, 2);

        let results = store.content_search("authentication").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "notes.md");
    }
}
```

- [ ] **Step 3: Add `pub mod content;` to mod.rs**

In `src-tauri/src/graph/mod.rs`, add after the existing `pub mod llm;`:

```rust
pub mod content;
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test graph::content -- --nocapture 2>&1 | tail -15
```

Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/content.rs src-tauri/src/graph/mod.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p2): content.rs — text extractor, binary detection, FTS folder indexer"
```

---

## Task 3: StructuredQuery ContentSearch Variant

**Files:**
- Modify: `src-tauri/src/graph/query.rs`

- [ ] **Step 1: Write failing test**

Add to the `tests` module in `src-tauri/src/graph/query.rs`:

```rust
#[test]
fn heuristic_parse_detects_content_search() {
    let q = heuristic_parse("find files containing TODO");
    assert!(matches!(q, StructuredQuery::ContentSearch { .. }));
}

#[test]
fn content_search_roundtrips_json() {
    let q = StructuredQuery::ContentSearch { terms: "authentication".into() };
    let json = serde_json::to_string(&q).unwrap();
    let q2: StructuredQuery = serde_json::from_str(&json).unwrap();
    assert!(matches!(q2, StructuredQuery::ContentSearch { .. }));
}
```

- [ ] **Step 2: Run to verify fail**

```bash
cd src-tauri && cargo test graph::query::tests::heuristic_parse_detects_content_search -- --nocapture 2>&1 | tail -5
```

Expected: compile error (`ContentSearch` variant doesn't exist).

- [ ] **Step 3: Add `ContentSearch` to StructuredQuery and update execute + heuristic_parse**

In `src-tauri/src/graph/query.rs`, modify `StructuredQuery` to add the new variant:

```rust
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
    ContentSearch {
        terms: String,
    },
}
```

Update `execute` to handle the new variant (add after the `GetRelated` arm):

```rust
        StructuredQuery::ContentSearch { terms } =>
            store.content_search(terms).map_err(|e| e.to_string()),
```

Update `heuristic_parse` to detect content search keywords:

```rust
pub fn heuristic_parse(input: &str) -> StructuredQuery {
    let lower = input.to_lowercase();

    if lower.contains("duplicate") || lower.contains("dupe") {
        return StructuredQuery::FindDuplicates { path: "/".into() };
    }

    // Content search keywords
    if lower.contains("containing") || lower.contains("content") || lower.contains("inside")
        || lower.contains("with text") || lower.contains("mentions")
    {
        let terms = input
            .split_whitespace()
            .filter(|w| !["find","files","containing","content","inside","with","text","mentions","that"].contains(&w.to_lowercase().as_str()))
            .collect::<Vec<_>>()
            .join(" ");
        return StructuredQuery::ContentSearch { terms: if terms.is_empty() { input.into() } else { terms } };
    }

    let extension = if lower.contains("video") || lower.contains(".mp4") { Some(".mp4".into()) }
        else if lower.contains("image") || lower.contains("photo") { Some(".jpg".into()) }
        else if lower.contains("pdf") { Some(".pdf".into()) }
        else { None };

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
```

- [ ] **Step 4: Run all query tests**

```bash
cd src-tauri && cargo test graph::query -- --nocapture 2>&1 | tail -15
```

Expected: 7 tests pass (5 original + 2 new).

- [ ] **Step 5: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/query.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p2): ContentSearch query variant, heuristic detection"
```

---

## Task 4: New Tauri Commands + graph_search Upgrade

**Files:**
- Modify: `src-tauri/src/graph/mod.rs`

- [ ] **Step 1: Add 4 new commands to mod.rs**

Add these 4 commands after `graph_set_llm` in `src-tauri/src/graph/mod.rs`:

```rust
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

    // Trigger content indexing in background — no lock held during extraction
    let store_arc = state.store.clone();
    std::thread::spawn(move || {
        let (entries, nodes) = crate::graph::indexer::collect_nodes(std::path::Path::new(&path));
        // Ensure nodes are in the store first (may already be from phase 1)
        if let Ok(store) = store_arc.lock() {
            let _ = store.conn.execute("BEGIN", []);
            for node in &nodes { let _ = store.upsert_node(node); }
            let _ = store.conn.execute("COMMIT", []);
        }
        // Extract and index content
        if let Ok(store) = store_arc.lock() {
            crate::graph::content::index_folder(std::path::Path::new(&path), &store);
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
```

- [ ] **Step 2: Upgrade graph_search to merge content results**

Replace the existing `graph_search` command with this merged version:

```rust
#[tauri::command]
pub async fn graph_search(
    query_str: String,
    state: State<'_, GraphAppState>,
) -> Result<Vec<SearchResult>, String> {
    let q = resolve_query(&query_str, &state).await;

    // Run metadata/graph query
    let mut results = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        execute(&q, &store)?
    };

    // If metadata returned nothing, fall through to content search
    if results.is_empty() {
        if let Ok(store) = state.store.lock() {
            if let Ok(content_results) = store.content_search(&query_str) {
                results = content_results;
            }
        }
    }

    Ok(results)
}
```

- [ ] **Step 3: Register new commands in lib.rs**

In `src-tauri/src/lib.rs`, find the `invoke_handler` block and add after `graph::graph_set_llm`:

```rust
graph::graph_add_indexed_folder,
graph::graph_remove_indexed_folder,
graph::graph_list_indexed_folders,
graph::graph_content_search,
```

- [ ] **Step 4: Build to verify**

```bash
cd src-tauri && cargo build --lib 2>&1 | grep "^error" | head -10
```

Expected: no errors.

- [ ] **Step 5: Run all graph tests**

```bash
cd src-tauri && cargo test graph:: 2>&1 | tail -8
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/mod.rs src-tauri/src/lib.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p2): 4 content commands, graph_search falls through to FTS"
```

---

## Task 5: Frontend API Bindings + graphui Upgrade

**Files:**
- Modify: `frontend/js/graph.ts`
- Modify: `frontend/js/graphui.ts`

- [ ] **Step 1: Add 4 new API functions to graph.ts**

Add after `graphSetLlm` in `frontend/js/graph.ts`:

```typescript
export async function graphAddIndexedFolder(path: string): Promise<void> {
    return _invoke('graph_add_indexed_folder', { path }) as Promise<void>;
}

export async function graphRemoveIndexedFolder(path: string): Promise<void> {
    return _invoke('graph_remove_indexed_folder', { path }) as Promise<void>;
}

export async function graphListIndexedFolders(): Promise<string[]> {
    return _invoke('graph_list_indexed_folders') as Promise<string[]>;
}

export async function graphContentSearch(query: string): Promise<GraphSearchResult[]> {
    return _invoke('graph_content_search', { query }) as Promise<GraphSearchResult[]>;
}
```

- [ ] **Step 2: Add indexed folders panel to graphui.ts**

Add these functions to `frontend/js/graphui.ts`:

```typescript
// ─── Indexed Folders Panel ────────────────────────────────────────────────────

export async function addIndexedFolder(path: string): Promise<void> {
    try {
        await graphApi.graphAddIndexedFolder(path);
        toast(`Indexing content in ${path.split('/').pop()}…`, '');
        await refreshIndexedFoldersList();
    } catch (e) {
        toast(`Failed to index folder: ${e}`, 'error');
    }
}

export async function refreshIndexedFoldersList(): Promise<void> {
    const list = document.getElementById('graph-indexed-folders-list');
    if (!list) return;

    let folders: string[] = [];
    try { folders = await graphApi.graphListIndexedFolders(); } catch { return; }

    list.innerHTML = folders.length === 0
        ? '<div class="graph-no-folders">No folders deep-indexed yet</div>'
        : folders.map(f => `
            <div class="graph-indexed-folder" data-path="${_escHtml(f)}">
                <span class="gif-name" title="${_escHtml(f)}">${_escHtml(f.split('/').pop() || f)}</span>
                <button class="gif-remove" data-path="${_escHtml(f)}" title="Remove">✕</button>
            </div>
          `).join('');

    list.querySelectorAll<HTMLElement>('.gif-remove').forEach(btn => {
        btn.addEventListener('click', async () => {
            const p = btn.dataset.path!;
            try {
                await graphApi.graphRemoveIndexedFolder(p);
                await refreshIndexedFoldersList();
            } catch (e) {
                toast(`Remove failed: ${e}`, 'error');
            }
        });
    });
}
```

Also update `initGraphUI` to call `refreshIndexedFoldersList()` at startup. Add this line inside `initGraphUI`, after `_startStatusPolling()`:

```typescript
refreshIndexedFoldersList();
```

Also listen for content-indexed events — add to `initGraphUI`:

```typescript
const tauri = (window as any).__TAURI__;
if (tauri?.event?.listen) {
    tauri.event.listen('graph-content-indexed', () => {
        refreshIndexedFoldersList();
        toast('Content indexing complete', 'success');
    });
}
```

- [ ] **Step 3: Compile TypeScript**

```bash
npm run build:ts 2>&1
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git -C /Users/suraj/Documents/tracer add frontend/js/graph.ts frontend/js/graph.js frontend/js/graphui.ts frontend/js/graphui.js
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p2): frontend API bindings and indexed folders panel"
```

---

## Task 6: HTML + CSS — Deep Index Menu Item and Settings Panel

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/css/style.css`
- Modify: `frontend/js/events.ts`

- [ ] **Step 1: Add "Deep Index" context menu item to index.html**

In `frontend/index.html`, find the context menu (`<div id="ctx-menu">`) and add after the `ctx-new-folder` item:

```html
<div class="ctx-separator"></div>
<div class="ctx-item" id="ctx-deep-index">⬇ Deep Index This Folder</div>
```

Add the indexed folders settings panel before the closing `</body>` tag:

```html
<!-- Graph indexed folders panel -->
<div id="graph-indexed-panel" class="hidden">
    <div class="graph-indexed-header">
        <span>Deep-Indexed Folders</span>
        <button id="graph-indexed-close">✕</button>
    </div>
    <div id="graph-indexed-folders-list"></div>
</div>
```

- [ ] **Step 2: Add CSS for new elements**

Append to `frontend/css/style.css`:

```css
/* ── Deep index panel ───────────────────────────────────────────── */
#graph-indexed-panel {
    position: fixed;
    bottom: 40px;
    right: 16px;
    width: 300px;
    background: var(--bg-panel);
    border: 1px solid var(--border-hi);
    border-radius: var(--radius);
    box-shadow: 0 4px 20px rgba(0,0,0,0.5);
    z-index: 150;
    overflow: hidden;
}

.graph-indexed-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px;
    font-size: 0.75rem;
    color: var(--text-dim);
    border-bottom: 1px solid var(--border);
}

.graph-indexed-header button {
    background: none;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
}

.graph-indexed-folder {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 12px;
    font-size: 0.78rem;
    border-bottom: 1px solid var(--border);
}

.gif-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
}

.gif-remove {
    background: none;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
    padding: 0 4px;
    font-size: 0.72rem;
    flex-shrink: 0;
}

.gif-remove:hover { color: var(--danger); }

.graph-no-folders {
    padding: 16px 12px;
    font-size: 0.78rem;
    color: var(--text-dim);
    text-align: center;
}

/* ── Snippet display in results ─────────────────────────────────── */
.gr-snippet {
    grid-column: 2 / -1;
    font-size: 0.72rem;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-style: italic;
}
```

- [ ] **Step 3: Wire "Deep Index" in events.ts**

In `frontend/js/events.ts`, add this import at the top (after existing imports):

```typescript
import { addIndexedFolder, refreshIndexedFoldersList } from './graphui.js';
```

In `bindGlobalEvents()`, add the ctx-deep-index handler after the existing context menu handlers:

```typescript
document.getElementById('ctx-deep-index')?.addEventListener('click', async () => {
    document.getElementById('ctx-menu')!.classList.add('hidden');
    const item = state.ctxTarget;
    if (!item || item.type !== 'directory') return;
    await addIndexedFolder(item.path);
});

document.getElementById('graph-indexed-close')?.addEventListener('click', () => {
    document.getElementById('graph-indexed-panel')?.classList.add('hidden');
});
```

Also add `ctx-deep-index` to the context menu visibility logic so it only shows for directories. Find where `bindNodeContextMenu` is defined in `events.ts` and update it to show/hide `ctx-deep-index` based on node type. Find the existing `bindNodeContextMenu` function and add:

```typescript
const deepIndexItem = document.getElementById('ctx-deep-index');
if (deepIndexItem) deepIndexItem.style.display = item.type === 'directory' ? '' : 'none';
```

- [ ] **Step 4: Compile and verify**

```bash
npm run build:ts 2>&1
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git -C /Users/suraj/Documents/tracer add frontend/index.html frontend/css/style.css frontend/js/events.ts frontend/js/events.js
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p2): deep index context menu, indexed folders panel, snippet CSS"
```

---

## Task 7: Final Verification

**Files:** none (verification only)

- [ ] **Step 1: Run full test suite**

```bash
cd src-tauri && cargo test 2>&1 | tail -10
```

Expected: all tests pass (26+ from Phase 1 + 7 new from Phase 2 = 33+).

- [ ] **Step 2: Clippy**

```bash
cd src-tauri && cargo clippy --lib 2>&1 | grep "^error" | head -10
```

Expected: no errors.

- [ ] **Step 3: TypeScript compile**

```bash
npm run build:ts 2>&1
```

Expected: no errors.

- [ ] **Step 4: Smoke test in dev server**

```bash
npm run tauri dev
```

Test sequence:
1. Right-click a folder → "Deep Index This Folder" appears
2. Click it → toast "Indexing content in [name]…"
3. After indexing completes → toast "Content indexing complete"
4. Switch to Search tab → type a term that appears in a file in that folder
5. Results appear with italic snippet text showing where the term matched
6. Type "find files containing TODO" → returns files with TODO in content

- [ ] **Step 5: Final commit**

```bash
git -C /Users/suraj/Documents/tracer add -A
git -C /Users/suraj/Documents/tracer commit -m "feat: knowledge graph Phase 2 complete — FTS5 content search, opt-in folder indexing" --allow-empty
```
