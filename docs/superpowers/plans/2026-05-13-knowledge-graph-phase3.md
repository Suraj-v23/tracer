# Knowledge Graph — Phase 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Parse import/require/use statements from code files, store `imports` edges in SQLite, and expose Tauri commands to query forward deps, reverse deps, and full dependency trees.

**Architecture:** New `parser.rs` extracts import paths per language using regex (no AST). `store.rs` gains query methods for import edges. `mod.rs` adds 3 new commands. `graph_add_indexed_folder` is upgraded to also run the import parser after content indexing. Frontend adds "Show Imports" / "Show Importers" to the sidebar and a dep-tree toggle on the canvas.

**Tech Stack:** `regex` crate (new dep), `petgraph` (existing), SQLite edges table (existing, `kind='imports'`), TypeScript (existing pipeline).

---

## File Map

**Create (Rust):**
- `src-tauri/src/graph/parser.rs` — per-language import extractor

**Modify (Rust):**
- `src-tauri/Cargo.toml` — add `regex = "1"`
- `src-tauri/src/graph/store.rs` — add `get_imports`, `get_importers`, `get_dep_tree` query methods
- `src-tauri/src/graph/mod.rs` — add `pub mod parser`, 3 new commands, wire parser into `graph_add_indexed_folder`
- `src-tauri/src/graph/query.rs` — add `GetImports` / `GetImporters` StructuredQuery variants

**Modify (Frontend):**
- `frontend/js/graph.ts` — add 3 new API bindings
- `frontend/js/graphui.ts` — add import/importer display to results
- `frontend/index.html` — add "Show Imports" / "Show Importers" context menu items
- `frontend/js/events.ts` — wire new context menu items

---

## Task 1: Add regex dependency

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add regex to Cargo.toml**

Add to `[dependencies]` in `src-tauri/Cargo.toml`:
```toml
regex = "1"
```

- [ ] **Step 2: Verify**

```bash
cd src-tauri && cargo fetch 2>&1 | tail -3
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/Cargo.toml
git -C /Users/suraj/Documents/tracer commit -m "chore(graph/p3): add regex dependency"
```

---

## Task 2: parser.rs — per-language import extractor

**Files:**
- Create: `src-tauri/src/graph/parser.rs`

- [ ] **Step 1: Create the file**

Create `src-tauri/src/graph/parser.rs` with this exact content:

```rust
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

// ─── Regex patterns (compiled once) ─────────────────────────────────────────

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("invalid regex")
}

static TS_IMPORT:   OnceLock<Regex> = OnceLock::new();
static JS_REQUIRE:  OnceLock<Regex> = OnceLock::new();
static PY_IMPORT:   OnceLock<Regex> = OnceLock::new();
static PY_FROM:     OnceLock<Regex> = OnceLock::new();
static RS_USE:      OnceLock<Regex> = OnceLock::new();
static RS_MOD:      OnceLock<Regex> = OnceLock::new();
static GO_IMPORT:   OnceLock<Regex> = OnceLock::new();
static CSS_IMPORT:  OnceLock<Regex> = OnceLock::new();
static HTML_LINK:   OnceLock<Regex> = OnceLock::new();
static HTML_SCRIPT: OnceLock<Regex> = OnceLock::new();

// ─── Public API ───────────────────────────────────────────────────────────────

/// Extract raw import path strings from source text.
/// Returns relative or module paths as written in the source — NOT resolved.
pub fn extract_imports(text: &str, extension: &str) -> Vec<String> {
    match extension {
        ".ts" | ".tsx" | ".js" | ".jsx" | ".mjs" => extract_js(text),
        ".py"                                     => extract_py(text),
        ".rs"                                     => extract_rs(text),
        ".go"                                     => extract_go(text),
        ".css" | ".scss" | ".sass"                => extract_css(text),
        ".html" | ".htm"                          => extract_html(text),
        _                                         => vec![],
    }
}

/// Resolve a raw import path to an absolute filesystem path.
/// `source_file` is the absolute path of the file doing the importing.
/// Returns None if the import is a bare module name (e.g. "react", "std::io").
pub fn resolve_import(raw: &str, source_file: &Path) -> Option<String> {
    // Bare module names (no ./ or ../) are not filesystem paths
    if !raw.starts_with('.') {
        return None;
    }

    let dir = source_file.parent()?;
    let resolved = dir.join(raw);

    // Try exact path first
    if resolved.exists() {
        return Some(resolved.to_string_lossy().to_string());
    }

    // Try adding common extensions
    for ext in &[".ts", ".tsx", ".js", ".jsx", ".py", ".rs"] {
        let with_ext = dir.join(format!("{raw}{ext}"));
        if with_ext.exists() {
            return Some(with_ext.to_string_lossy().to_string());
        }
    }

    // Try as directory index
    for index in &["index.ts", "index.js", "mod.rs"] {
        let index_path = resolved.join(index);
        if index_path.exists() {
            return Some(index_path.to_string_lossy().to_string());
        }
    }

    None
}

// ─── Per-language extractors ─────────────────────────────────────────────────

fn extract_js(text: &str) -> Vec<String> {
    let mut out = Vec::new();

    // import ... from '...' / import '...'
    let ts = TS_IMPORT.get_or_init(|| re(r#"(?m)import\s+(?:[^'"]*\s+from\s+)?['"]([^'"]+)['"]"#));
    for cap in ts.captures_iter(text) {
        out.push(cap[1].to_string());
    }

    // require('...')
    let req = JS_REQUIRE.get_or_init(|| re(r#"require\s*\(\s*['"]([^'"]+)['"]\s*\)"#));
    for cap in req.captures_iter(text) {
        out.push(cap[1].to_string());
    }

    out
}

fn extract_py(text: &str) -> Vec<String> {
    let mut out = Vec::new();

    // import foo.bar → treat as module name, skip (not a relative path)
    // from .foo import bar → relative, keep
    let from = PY_FROM.get_or_init(|| re(r"(?m)^from\s+(\.\.?[\w./]*)\s+import"));
    for cap in from.captures_iter(text) {
        let raw = &cap[1];
        // Convert Python dotted relative to path-like: ".foo" → "./foo"
        let path = raw.replacen('.', "./", 1).replace('.', "/");
        out.push(path);
    }

    // import foo — only keep if it starts with relative indicator
    let imp = PY_IMPORT.get_or_init(|| re(r"(?m)^import\s+([\w.]+)"));
    for cap in imp.captures_iter(text) {
        let raw = cap[1].to_string();
        if raw.starts_with('.') {
            out.push(raw);
        }
    }

    out
}

fn extract_rs(text: &str) -> Vec<String> {
    let mut out = Vec::new();

    // mod foo; → sibling file foo.rs
    let mod_re = RS_MOD.get_or_init(|| re(r"(?m)^\s*(?:pub\s+)?mod\s+(\w+)\s*;"));
    for cap in mod_re.captures_iter(text) {
        out.push(format!("./{}", &cap[1]));
    }

    // use super:: / use self:: (relative) — skip crate:: / std:: (external)
    let use_re = RS_USE.get_or_init(|| re(r"(?m)^\s*use\s+((?:super|self)::[\w:]+)"));
    for cap in use_re.captures_iter(text) {
        let raw = cap[1].replace("::", "/").replace("super", "..");
        out.push(raw);
    }

    out
}

fn extract_go(text: &str) -> Vec<String> {
    let mut out = Vec::new();

    // import "path/to/pkg" or import ( "..." )
    let re = GO_IMPORT.get_or_init(|| re(r#"import\s+(?:"([^"]+)"|(?:\([^)]*\)))"#));
    for cap in re.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let p = m.as_str().to_string();
            // Only keep relative-looking paths (start with ./ or contain /)
            if p.starts_with('.') {
                out.push(p);
            }
        }
    }

    // Also scan multi-line import blocks
    let block = Regex::new(r#""(\.[\w./]+)""#).unwrap();
    for cap in block.captures_iter(text) {
        out.push(cap[1].to_string());
    }

    out
}

fn extract_css(text: &str) -> Vec<String> {
    let re = CSS_IMPORT.get_or_init(|| re(r#"@import\s+['"]([^'"]+)['"]"#));
    re.captures_iter(text).map(|c| c[1].to_string()).collect()
}

fn extract_html(text: &str) -> Vec<String> {
    let mut out = Vec::new();

    let link = HTML_LINK.get_or_init(|| re(r#"<link[^>]+href=['"]([^'"]+\.css)['"]"#));
    for cap in link.captures_iter(text) { out.push(cap[1].to_string()); }

    let script = HTML_SCRIPT.get_or_init(|| re(r#"<script[^>]+src=['"]([^'"]+)['"]"#));
    for cap in script.captures_iter(text) { out.push(cap[1].to_string()); }

    out
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_import_from() {
        let src = r#"import { foo } from './utils'
import type { Bar } from '../types'
import 'side-effect'"#;
        let imports = extract_imports(src, ".ts");
        assert!(imports.contains(&"./utils".to_string()));
        assert!(imports.contains(&"../types".to_string()));
        assert!(imports.contains(&"side-effect".to_string()));
    }

    #[test]
    fn js_require() {
        let src = r#"const x = require('./lib')
const y = require("../config")"#;
        let imports = extract_imports(src, ".js");
        assert!(imports.contains(&"./lib".to_string()));
        assert!(imports.contains(&"../config".to_string()));
    }

    #[test]
    fn py_relative_from() {
        let src = "from .models import User\nfrom ..utils import helper";
        let imports = extract_imports(src, ".py");
        assert_eq!(imports.len(), 2);
    }

    #[test]
    fn rs_mod_declaration() {
        let src = "pub mod store;\nmod query;\nuse std::io;";
        let imports = extract_imports(src, ".rs");
        assert!(imports.contains(&"./store".to_string()));
        assert!(imports.contains(&"./query".to_string()));
        // std:: should not appear (external crate)
        assert!(!imports.iter().any(|s| s.contains("std")));
    }

    #[test]
    fn css_import() {
        let src = r#"@import './variables.css'
@import "../reset.css""#;
        let imports = extract_imports(src, ".css");
        assert_eq!(imports.len(), 2);
    }

    #[test]
    fn html_links_and_scripts() {
        let src = r#"<link rel="stylesheet" href="./style.css">
<script src="../js/app.js"></script>"#;
        let imports = extract_imports(src, ".html");
        assert!(imports.iter().any(|s| s.contains("style.css")));
        assert!(imports.iter().any(|s| s.contains("app.js")));
    }

    #[test]
    fn resolve_relative_path_adds_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("utils.ts"), b"export {}").unwrap();

        let source = dir.path().join("main.ts");
        let resolved = resolve_import("./utils", &source);
        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("utils.ts"));
    }

    #[test]
    fn resolve_bare_module_returns_none() {
        let source = std::path::Path::new("/project/src/main.ts");
        assert!(resolve_import("react", source).is_none());
        assert!(resolve_import("std::io", source).is_none());
    }

    #[test]
    fn unknown_extension_returns_empty() {
        let imports = extract_imports("whatever content", ".exe");
        assert!(imports.is_empty());
    }
}
```

- [ ] **Step 2: Add `pub mod parser;` to mod.rs**

Add after `pub mod content;` in `src-tauri/src/graph/mod.rs`:
```rust
pub mod parser;
```

- [ ] **Step 3: Run tests**

```bash
cd src-tauri && cargo test graph::parser -- --nocapture 2>&1 | tail -15
```

Expected: 9 tests pass.

- [ ] **Step 4: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/parser.rs src-tauri/src/graph/mod.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p3): parser.rs — per-language import extractor with path resolution"
```

---

## Task 3: store.rs — import edge queries

**Files:**
- Modify: `src-tauri/src/graph/store.rs`

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)]` block in `src-tauri/src/graph/store.rs`:

```rust
#[test]
fn get_imports_returns_direct_deps() {
    let store = Store::open_in_memory().unwrap();
    store.upsert_node(&make_node("/a/main.ts",  "main.ts",  "file", 100)).unwrap();
    store.upsert_node(&make_node("/a/utils.ts", "utils.ts", "file", 50)).unwrap();
    store.upsert_node(&make_node("/a/types.ts", "types.ts", "file", 30)).unwrap();
    store.upsert_edge("/a/main.ts", "/a/utils.ts", "imports").unwrap();
    store.upsert_edge("/a/main.ts", "/a/types.ts", "imports").unwrap();

    let imports = store.get_imports("/a/main.ts").unwrap();
    assert_eq!(imports.len(), 2);
    let names: Vec<_> = imports.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"utils.ts"));
    assert!(names.contains(&"types.ts"));
}

#[test]
fn get_importers_returns_reverse_deps() {
    let store = Store::open_in_memory().unwrap();
    store.upsert_node(&make_node("/a/main.ts",  "main.ts",  "file", 100)).unwrap();
    store.upsert_node(&make_node("/a/lib.ts",   "lib.ts",   "file", 100)).unwrap();
    store.upsert_node(&make_node("/a/utils.ts", "utils.ts", "file",  50)).unwrap();
    store.upsert_edge("/a/main.ts", "/a/utils.ts", "imports").unwrap();
    store.upsert_edge("/a/lib.ts",  "/a/utils.ts", "imports").unwrap();

    let importers = store.get_importers("/a/utils.ts").unwrap();
    assert_eq!(importers.len(), 2);
    let names: Vec<_> = importers.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"main.ts"));
    assert!(names.contains(&"lib.ts"));
}

#[test]
fn import_count_on_node() {
    let store = Store::open_in_memory().unwrap();
    store.upsert_node(&make_node("/a/main.ts",  "main.ts",  "file", 100)).unwrap();
    store.upsert_node(&make_node("/a/utils.ts", "utils.ts", "file",  50)).unwrap();
    store.upsert_edge("/a/main.ts", "/a/utils.ts", "imports").unwrap();

    assert_eq!(store.import_count("/a/main.ts").unwrap(),   1); // outgoing
    assert_eq!(store.importer_count("/a/utils.ts").unwrap(), 1); // incoming
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cd src-tauri && cargo test graph::store::tests::get_imports -- --nocapture 2>&1 | tail -5
```

Expected: compile error (methods don't exist).

- [ ] **Step 3: Add methods to Store impl**

Add after the `is_folder_indexed` method in `src-tauri/src/graph/store.rs`:

```rust
// ── Import Graph Queries ──────────────────────────────────────────────────

pub fn get_imports(&self, path: &str) -> SqlResult<Vec<SearchResult>> {
    let mut stmt = self.conn.prepare(r#"
        SELECT n2.path, n2.name, n2.kind, n2.size, n2.extension, n2.modified_secs
        FROM nodes n1
        JOIN edges e  ON e.from_id = n1.id AND e.kind = 'imports'
        JOIN nodes n2 ON n2.id = e.to_id
        WHERE n1.path = ?1
        ORDER BY n2.name
        LIMIT 200
    "#)?;
    let results = stmt.query_map(params![path], row_to_result)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(results)
}

pub fn get_importers(&self, path: &str) -> SqlResult<Vec<SearchResult>> {
    let mut stmt = self.conn.prepare(r#"
        SELECT n2.path, n2.name, n2.kind, n2.size, n2.extension, n2.modified_secs
        FROM nodes n1
        JOIN edges e  ON e.to_id = n1.id AND e.kind = 'imports'
        JOIN nodes n2 ON n2.id = e.from_id
        WHERE n1.path = ?1
        ORDER BY n2.name
        LIMIT 200
    "#)?;
    let results = stmt.query_map(params![path], row_to_result)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(results)
}

pub fn import_count(&self, path: &str) -> SqlResult<i64> {
    self.conn.query_row(
        "SELECT COUNT(*) FROM nodes n JOIN edges e ON e.from_id=n.id AND e.kind='imports' WHERE n.path=?1",
        params![path],
        |r| r.get(0),
    )
}

pub fn importer_count(&self, path: &str) -> SqlResult<i64> {
    self.conn.query_row(
        "SELECT COUNT(*) FROM nodes n JOIN edges e ON e.to_id=n.id AND e.kind='imports' WHERE n.path=?1",
        params![path],
        |r| r.get(0),
    )
}
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test graph::store -- --nocapture 2>&1 | tail -10
```

Expected: all 13 tests pass (10 original + 3 new).

- [ ] **Step 5: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/store.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p3): store import edge queries — get_imports, get_importers, counts"
```

---

## Task 4: Index imports in graph_add_indexed_folder

**Files:**
- Modify: `src-tauri/src/graph/mod.rs`

- [ ] **Step 1: Write a test for the indexing pipeline**

Add to `src-tauri/src/graph/parser.rs` tests:

```rust
#[test]
fn index_imports_for_folder() {
    use crate::graph::store::{GraphNode, Store};
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    let main_path = dir.path().join("main.ts");
    let utils_path = dir.path().join("utils.ts");
    fs::write(&main_path, b"import { foo } from './utils'").unwrap();
    fs::write(&utils_path, b"export function foo() {}").unwrap();

    let store = Store::open_in_memory().unwrap();
    store.upsert_node(&GraphNode { id: 0,
        path: main_path.to_string_lossy().to_string(),
        name: "main.ts".into(), kind: "file".into(), size: 28,
        extension: Some(".ts".into()), modified_secs: None, created_secs: None, content_hash: None,
    }).unwrap();
    store.upsert_node(&GraphNode { id: 0,
        path: utils_path.to_string_lossy().to_string(),
        name: "utils.ts".into(), kind: "file".into(), size: 24,
        extension: Some(".ts".into()), modified_secs: None, created_secs: None, content_hash: None,
    }).unwrap();

    index_imports(dir.path(), &store);

    let imports = store.get_imports(&main_path.to_string_lossy()).unwrap();
    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].name, "utils.ts");
}
```

Also add `pub fn index_imports` to `parser.rs`:

```rust
/// Walk all code files under `root` that are in the node store,
/// parse their imports, resolve paths, and insert `imports` edges.
pub fn index_imports(root: &Path, store: &crate::graph::store::Store) {
    use walkdir::WalkDir;
    use rayon::prelude::*;

    let entries: Vec<_> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .collect();

    // Extract (source_path, Vec<resolved_target_path>) in parallel
    let pairs: Vec<(String, Vec<String>)> = entries.par_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let ext = path.extension()
                .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                .unwrap_or_default();
            let text = std::fs::read_to_string(path).ok()?;
            let raw_imports = extract_imports(&text, &ext);
            if raw_imports.is_empty() { return None; }

            let resolved: Vec<String> = raw_imports.iter()
                .filter_map(|raw| resolve_import(raw, path))
                .collect();
            if resolved.is_empty() { return None; }

            Some((path.to_string_lossy().to_string(), resolved))
        })
        .collect();

    // Write edges sequentially (SQLite single-writer)
    let _ = store.conn.execute("BEGIN", []);
    for (from_path, targets) in &pairs {
        for to_path in targets {
            let _ = store.upsert_edge(from_path, to_path, "imports");
        }
    }
    let _ = store.conn.execute("COMMIT", []);
}
```

- [ ] **Step 2: Run the new parser test**

```bash
cd src-tauri && cargo test graph::parser -- --nocapture 2>&1 | tail -15
```

Expected: 10 tests pass.

- [ ] **Step 3: Wire index_imports into graph_add_indexed_folder in mod.rs**

Find the `graph_add_indexed_folder` command in `src-tauri/src/graph/mod.rs`. After the `content::index_folder` call, add import indexing:

```rust
        // Extract and index content (lock released between phases)
        if let Ok(store) = store_arc.lock() {
            crate::graph::content::index_folder(std::path::Path::new(&path), &store);
        }
        // Parse and store import edges
        if let Ok(store) = store_arc.lock() {
            crate::graph::parser::index_imports(std::path::Path::new(&path), &store);
        }
        app.emit("graph-content-indexed", &path).ok();
```

Replace only the line `app.emit("graph-content-indexed", &path).ok();` and the content indexing line with the block above. The full spawn closure section should look like this after the node upsert loop:

```rust
        // Extract and index content (lock released between phases)
        if let Ok(store) = store_arc.lock() {
            crate::graph::content::index_folder(std::path::Path::new(&path), &store);
        }
        // Parse and store import edges
        if let Ok(store) = store_arc.lock() {
            crate::graph::parser::index_imports(std::path::Path::new(&path), &store);
        }
        app.emit("graph-content-indexed", &path).ok();
```

- [ ] **Step 4: Build**

```bash
cd src-tauri && cargo build --lib 2>&1 | grep "^error" | head -10
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/parser.rs src-tauri/src/graph/mod.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p3): index_imports wired into folder indexing pipeline"
```

---

## Task 5: 3 new Tauri commands + StructuredQuery variants

**Files:**
- Modify: `src-tauri/src/graph/mod.rs`
- Modify: `src-tauri/src/graph/query.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add DepTree type and 3 commands to mod.rs**

Add after `graph_content_search` in `src-tauri/src/graph/mod.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DepTree {
    pub path:     String,
    pub name:     String,
    pub imports:  Vec<DepTree>,
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
```

- [ ] **Step 2: Register commands in lib.rs**

In `src-tauri/src/lib.rs`, add after `graph::graph_content_search`:

```rust
graph::graph_get_imports,
graph::graph_get_importers,
graph::graph_get_dep_tree,
```

- [ ] **Step 3: Add GetImports / GetImporters to StructuredQuery in query.rs**

Add two variants to the `StructuredQuery` enum (after `ContentSearch`):

```rust
    GetImports {
        path: String,
    },
    GetImporters {
        path: String,
    },
```

Add match arms to `execute()`:

```rust
        StructuredQuery::GetImports { path } =>
            store.get_imports(path).map_err(|e| e.to_string()),

        StructuredQuery::GetImporters { path } =>
            store.get_importers(path).map_err(|e| e.to_string()),
```

Update `heuristic_parse` to detect import queries. Add before the `ContentSearch` detection block:

```rust
    // Import/dependency queries
    if (lower.contains("import") || lower.contains("depend") || lower.contains("use"))
        && lower.contains("this")
    {
        return StructuredQuery::GetImporters { path: String::new() };
    }
    if lower.contains("what does") && (lower.contains("import") || lower.contains("depend")) {
        return StructuredQuery::GetImports { path: String::new() };
    }
```

- [ ] **Step 4: Build and run all tests**

```bash
cd src-tauri && cargo build --lib 2>&1 | grep "^error" | head -5
cd src-tauri && cargo test --lib 2>&1 | tail -5
```

Expected: no errors, all tests pass.

- [ ] **Step 5: Commit**

```bash
git -C /Users/suraj/Documents/tracer add src-tauri/src/graph/mod.rs src-tauri/src/graph/query.rs src-tauri/src/lib.rs
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p3): graph_get_imports, graph_get_importers, graph_get_dep_tree commands"
```

---

## Task 6: Frontend — API bindings + context menu + sidebar counts

**Files:**
- Modify: `frontend/js/graph.ts`
- Modify: `frontend/index.html`
- Modify: `frontend/js/events.ts`
- Modify: `frontend/js/graphui.ts`

- [ ] **Step 1: Add 3 API functions + DepTree type to graph.ts**

Add after `graphContentSearch` in `frontend/js/graph.ts`:

```typescript
export interface DepTree {
    path:    string;
    name:    string;
    imports: DepTree[];
}

export async function graphGetImports(path: string): Promise<GraphSearchResult[]> {
    return _invoke('graph_get_imports', { path }) as Promise<GraphSearchResult[]>;
}

export async function graphGetImporters(path: string): Promise<GraphSearchResult[]> {
    return _invoke('graph_get_importers', { path }) as Promise<GraphSearchResult[]>;
}

export async function graphGetDepTree(path: string, depth?: number): Promise<DepTree> {
    return _invoke('graph_get_dep_tree', { path, depth }) as Promise<DepTree>;
}
```

- [ ] **Step 2: Add context menu items to index.html**

In `frontend/index.html`, find `<div class="ctx-item" id="ctx-deep-index">` and add AFTER it:

```html
<div class="ctx-item" id="ctx-show-imports">→ Show Imports</div>
<div class="ctx-item" id="ctx-show-importers">← Show Importers</div>
```

- [ ] **Step 3: Add showImportResults to graphui.ts**

Add this function after `addIndexedFolder` in `frontend/js/graphui.ts`:

```typescript
export async function showImports(path: string, mode: 'imports' | 'importers'): Promise<void> {
    const fn_ = mode === 'imports' ? graphApi.graphGetImports : graphApi.graphGetImporters;
    const label = mode === 'imports' ? 'imports' : 'imported by';
    const name = path.split('/').pop() || path;

    const panel = document.getElementById('graph-results-panel')!;
    panel.classList.remove('hidden');
    panel.innerHTML = '<div class="graph-results-loading">Loading…</div>';

    try {
        const results = await fn_(path);
        if (!results.length) {
            panel.innerHTML = `<div class="graph-results-empty">${_escHtml(name)} has no ${label}</div>`;
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
                <span>${_escHtml(name)} ${label} ${results.length} file${results.length !== 1 ? 's' : ''}</span>
                <button id="graph-results-close" class="graph-results-close">✕</button>
            </div>
            <div class="graph-results-list">${items}</div>
        `;
        document.getElementById('graph-results-close')?.addEventListener('click', hideResultsPanel);
    } catch (e) {
        panel.innerHTML = `<div class="graph-results-empty">Error: ${_escHtml(String(e))}</div>`;
    }
}
```

- [ ] **Step 4: Wire context menu items in events.ts**

Add import at top of `frontend/js/events.ts`:

```typescript
import { addIndexedFolder, showImports } from './graphui.js';
```

(Replace the existing `import { addIndexedFolder } from './graphui.js';` line.)

Add these handlers in `bindGlobalEvents()` after the `ctx-deep-index` handler:

```typescript
    document.getElementById('ctx-show-imports')?.addEventListener('click', async () => {
        document.getElementById('ctx-menu')!.classList.add('hidden');
        if (state.ctxTarget) await showImports(state.ctxTarget.path, 'imports');
    });

    document.getElementById('ctx-show-importers')?.addEventListener('click', async () => {
        document.getElementById('ctx-menu')!.classList.add('hidden');
        if (state.ctxTarget) await showImports(state.ctxTarget.path, 'importers');
    });
```

Also show/hide ctx-show-imports and ctx-show-importers for code files only. In `bindNodeContextMenu`, find where `deepIdx.style.display` is set and add:

```typescript
    const importItems = ['ctx-show-imports', 'ctx-show-importers'];
    const isCode = item.extension && ['.ts','.js','.tsx','.jsx','.py','.rs','.go'].includes(item.extension);
    importItems.forEach(id => {
        const el = document.getElementById(id);
        if (el) el.style.display = isCode ? '' : 'none';
    });
```

- [ ] **Step 5: Compile**

```bash
npm run build:ts 2>&1
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git -C /Users/suraj/Documents/tracer add frontend/js/graph.ts frontend/js/graph.js frontend/js/graphui.ts frontend/js/graphui.js frontend/index.html frontend/js/events.ts frontend/js/events.js
git -C /Users/suraj/Documents/tracer commit -m "feat(graph/p3): frontend import/importer context menu and results panel"
```

---

## Task 7: Final Verification

- [ ] **Step 1: Full test suite**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -8
```

Expected: all tests pass (36+ from Phase 1+2 + 13 new from Phase 3 = 49+).

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

Test:
1. Right-click a code folder → "Deep Index This Folder"
2. Wait for "Content indexing complete" toast
3. Right-click a `.ts` or `.rs` file → "Show Imports" → results panel shows files it imports
4. Right-click same file → "Show Importers" → shows files that import it
5. Search "find files containing TODO" → FTS results with snippets still work

- [ ] **Step 5: Final commit**

```bash
git -C /Users/suraj/Documents/tracer add -A
git -C /Users/suraj/Documents/tracer commit -m "feat: knowledge graph Phase 3 complete — code dependency graph" --allow-empty
```
