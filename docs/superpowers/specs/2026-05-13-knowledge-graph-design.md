# Knowledge Graph System — Design Spec
**Date:** 2026-05-13
**Project:** Tracer — Filesystem Graph Explorer
**Status:** Approved for implementation planning

---

## Overview

A 5-phase knowledge graph system that lets users and an AI model efficiently search and navigate any folder, project, or entire filesystem. Builds from simple metadata indexing up to full GraphRAG-style community query answering.

Each phase ships independently and adds value on its own. Later phases layer on top of earlier ones.

---

## Architecture

### Stack

| Layer | Technology | Purpose |
|---|---|---|
| Graph traversal | `petgraph` (in-memory) | BFS/DFS, dependency edges, cycle detection |
| Vector search | `sqlite-vec` + `usearch` HNSW | Semantic similarity, ANN queries |
| Full-text search | SQLite FTS5 | Content search, BM25 ranking |
| Metadata + edges | SQLite | Primary persistent store |
| File watching | `notify` crate | Incremental index updates |
| LLM bridge | `reqwest` (ollama / remote API) | NL→query, summarization, embeddings |

### Module Structure

```
src-tauri/src/graph/
  mod.rs          ← Tauri commands + GraphAppState
  store.rs        ← SQLite schema, migrations, read/write
  indexer.rs      ← parallel scanner (rayon) + notify file watcher
  query.rs        ← unified query engine (metadata/FTS/graph/vector)
  llm.rs          ← LLM bridge (ollama + remote API)
  parser.rs       ← import/reference extractor per language  [Phase 3]
  embedder.rs     ← embedding pipeline + usearch HNSW index  [Phase 4]
  community.rs    ← entity extraction, Leiden, summaries     [Phase 5]
```

### Data Flow

```
Filesystem
    ↓ indexer.rs (rayon parallel scan + notify watcher)
SQLite graph.db  ←→  petgraph (in-memory)
    ↓
query.rs  ←  llm.rs (NL → StructuredQuery)
    ↓
Tauri commands → Frontend
```

### SQLite Schema

```sql
-- Core nodes
CREATE TABLE nodes (
    id            INTEGER PRIMARY KEY,
    path          TEXT UNIQUE NOT NULL,
    name          TEXT NOT NULL,
    kind          TEXT NOT NULL,        -- 'file' | 'directory'
    size          INTEGER DEFAULT 0,
    extension     TEXT,
    modified_secs INTEGER,
    created_secs  INTEGER,
    content_hash  TEXT                  -- blake3, for duplicate detection
);
CREATE INDEX idx_nodes_size     ON nodes(size);
CREATE INDEX idx_nodes_ext      ON nodes(extension);
CREATE INDEX idx_nodes_modified ON nodes(modified_secs);

-- Graph edges
CREATE TABLE edges (
    from_id  INTEGER REFERENCES nodes(id),
    to_id    INTEGER REFERENCES nodes(id),
    kind     TEXT NOT NULL              -- 'parent' | 'duplicate' | 'imports'
);

-- Opt-in content-indexed folders
CREATE TABLE indexed_folders (
    path       TEXT PRIMARY KEY,
    added_secs INTEGER
);

-- FTS5 content index [Phase 2]
CREATE VIRTUAL TABLE fts_content USING fts5(
    path,
    content,
    content='nodes',
    content_rowid='id'
);

-- Vector embeddings [Phase 4]
CREATE VIRTUAL TABLE vec_embeddings USING vec0(
    node_id   INTEGER PRIMARY KEY,
    embedding FLOAT[384]
);

-- Community detection [Phase 5]
CREATE TABLE entities (
    id       INTEGER PRIMARY KEY,
    node_id  INTEGER REFERENCES nodes(id),
    name     TEXT NOT NULL,
    kind     TEXT,                      -- 'function' | 'class' | 'concept'
    weight   REAL DEFAULT 1.0
);
CREATE TABLE communities (
    id       INTEGER PRIMARY KEY,
    label    TEXT,
    summary  TEXT,
    member_ids TEXT                     -- JSON array of node ids
);

-- Settings
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT
);
```

### In-Memory State

```rust
pub struct GraphAppState {
    pub db:        Mutex<Connection>,
    pub graph:     RwLock<DiGraph<NodeId, EdgeKind>>,
    pub index:     RwLock<HashMap<String, NodeIndex>>,  // path → petgraph node
    pub llm:       RwLock<Option<LlmProvider>>,
    pub hnsw:      RwLock<Option<usearch::Index>>,      // Phase 4
}
```

---

## Phase 1 — Metadata Graph + File Watcher

**Goal:** Background scan, SQLite + petgraph, live updates, NL→filter queries.

### Indexer
- Parallel scan with `rayon` — fans out at directory level
- Computes `blake3` hash of file contents for duplicate detection
- Inserts `parent` edges for every file→directory relationship
- Inserts `duplicate` edges between files with identical hashes
- Runs in background thread, never blocks UI
- `notify` watcher debounces events (50ms window), batches updates

### LLM Bridge — Phase 1

NL query → LLM receives question + schema hint → returns JSON → parsed to `StructuredQuery`:

```rust
pub enum StructuredQuery {
    MetadataFilter { kind: Option<String>, size_gt: Option<u64>,
                     size_lt: Option<u64>, ext: Option<String>,
                     modified_after: Option<u64>, name_contains: Option<String> },
    ContentSearch  { terms: Vec<String> },           // Phase 2
    GraphTraversal { root_path: String, direction: Direction, depth: usize },
    SemanticSearch { query_text: String, k: usize }, // Phase 4
    GlobalQuestion { question: String },              // Phase 5
}
```

No free-form SQL generation — LLM fills structured fields only. Safe and predictable.

### Tauri Commands

```
graph_search(query: String)            → Vec<SearchResult>
graph_get_related(path: String)        → Vec<SearchResult>
graph_get_duplicates(path: String)     → Vec<SearchResult>
graph_index_status()                   → IndexStatus
graph_set_root(path: String)           → ()
```

---

## Phase 2 — Content Indexing (Opt-in)

**Goal:** FTS5 full-text search on file contents for user-selected folders.

### Supported File Types
```
Text/code: .ts .js .py .rs .go .md .txt .json .yaml .toml .html .css
Skip:      binaries, files > 2MB (configurable in settings)
```

### Extraction
- Runs on dedicated rayon thread pool (separate from metadata scanner)
- Extracts raw text → stores in `fts_content` FTS5 table
- Also extracts symbol hints (function/class names, import paths) → stored as tags for Phase 3

### Search Result Ranking
- FTS5 BM25 score merged with metadata relevance
- Results include matched content snippet via FTS5 `highlight()`

### New Tauri Commands
```
graph_add_indexed_folder(path: String)    → ()
graph_remove_indexed_folder(path: String) → ()
graph_list_indexed_folders()              → Vec<String>
graph_content_search(query: String)       → Vec<SearchResult>
```

---

## Phase 3 — Code Dependency Graph

**Goal:** Import/reference parsing → `imports` edges in petgraph. "What depends on this?"

### Import Parser (`parser.rs`)

Per-language regex extraction (no full AST — fast, sufficient for indexing):

| Language | Patterns |
|---|---|
| TypeScript/JS | `import ... from '...'`, `require('...')` |
| Python | `import ...`, `from ... import` |
| Rust | `use crate::...`, `mod ...` |
| Go | `import "..."` |
| CSS/HTML | `@import`, `<link href>`, `<script src>` |

Resolves relative paths to absolute → looks up in SQLite → inserts `imports` edge.

### Graph Queries
- Forward BFS: everything this file depends on
- Reverse BFS: everything that depends on this file (impact analysis)
- Cycle detection: circular imports flagged, emitted as `graph-cycle` Tauri event

### New Tauri Commands
```
graph_get_imports(path: String)       → Vec<SearchResult>
graph_get_importers(path: String)     → Vec<SearchResult>
graph_get_dep_tree(path: String)      → DepTree
```

---

## Phase 4 — Semantic Search

**Goal:** Embeddings + vector similarity. "Find files similar to this one."

### Embedding Pipeline (`embedder.rs`)
```
file content (from FTS index)
    ↓ chunk if > 512 tokens
    ↓ embed via provider:
        Local  → ollama (nomic-embed-text, all-minilm — 384 dims)
        Remote → OpenAI text-embedding-3-small | Voyage | Cohere
    ↓ store in vec_embeddings (sqlite-vec)
    ↓ build usearch HNSW index in-memory for ANN queries
```

### Performance
- sqlite-vec brute-force: <100ms for 100k vectors
- usearch HNSW: <50ms for 1M vectors
- Embedding generation runs lazily — only when user enables semantic search
- GPU acceleration used automatically if available via ollama

### New Tauri Commands
```
graph_semantic_search(query: String, k: usize)  → Vec<SearchResult>
graph_find_similar(path: String, k: usize)       → Vec<SearchResult>
graph_set_embedding_provider(config: LlmConfig)  → ()
```

### Unified Search
`graph_search` fans out to all engines, merges and ranks results:
```
query → metadata filter + FTS5 + vector similarity + graph traversal → ranked top-N
```

---

## Phase 5 — GraphRAG Community Layer

**Goal:** Entity extraction + community detection + LLM summaries. Answer "what does this codebase do?"

### Step 1 — Entity Extraction
Per file (uses content from Phase 2):
- Code files: function names, class names, exported symbols
- Docs/markdown: key concepts, nouns, topics (via LLM or regex)
- Stored in `entities` table with co-occurrence counts

### Step 2 — Community Detection
- Build entity co-occurrence graph (edges weighted by frequency)
- Run Leiden algorithm on entity graph
- Each community = cluster of semantically related files
- Stored in `communities` table

### Step 3 — Community Summarization
```
For each community:
  top files + entities → LLM prompt
  → summary stored in communities.summary
  → re-generated when community membership changes significantly
```

### Step 4 — Two-Level Query (GraphRAG)
```
NL question
    ↓ LLM maps → relevant communities (via summaries)
    ↓ drill into member files
    ↓ retrieve context
    ↓ LLM generates final answer with file citations
    → GlobalAnswer { answer: String, sources: Vec<SearchResult> }
```

### New Tauri Commands
```
graph_global_query(question: String)    → GlobalAnswer
graph_list_communities()                → Vec<Community>
graph_get_community(id: u64)            → Community
graph_rebuild_communities()             → ()
```

---

## LLM Configuration

Two providers, same interface:

```rust
pub enum LlmProvider {
    Ollama { base_url: String, model: String },
    Remote { endpoint: String, api_key: String, model: String },
}
```

Settings stored in `settings` table. User configures in app settings panel.

Fallback behavior: if LLM unavailable → metadata + FTS results returned, toast shown: *"AI offline — showing keyword results"*

---

## Frontend Changes

### Search Bar — Three Modes
```
[ Filter ▾ ]  [___search box___]  [Ask AI]
```
- **Filter** — existing behavior (dims nodes by name)
- **Search** — queries `graph_search`, shows results panel
- **Ask AI** — sends to `graph_global_query`, shows answer panel (Phase 5)

### New UI Panels
- **Results panel** — slides up from bottom, ranked file results with content snippets
- **Answer panel** — LLM answer + source files highlighted in canvas (Phase 5)
- **Communities tab** — sidebar tab, lists auto-detected clusters (Phase 5)
- **Index status bar** — bottom-right: "Indexing 24,310 / 180,000 files…"

### Context Menu Additions
- Right-click folder → "Deep Index This Folder" (Phase 2)
- Right-click file → "Show Dependents" (Phase 3)
- Right-click file → "Find Similar Files" (Phase 4)

### Sidebar Additions
- Code files: "Imported by X files" / "Imports Y files" (Phase 3)
- Folders: "Community: [name]" badge (Phase 5)

### Canvas Additions
- Toggle "show import edges" — additional wire color for dependency edges (Phase 3)
- Community bubble overlay — large translucent region wrapping member files (Phase 5)

---

## Error Handling

| Scenario | Behavior |
|---|---|
| Unreadable file during indexing | Skip + log, never crash |
| LLM unavailable | Fallback to metadata+FTS, toast shown |
| SQLite corruption | Delete + rebuild, user notified |
| File watcher event overflow | Debounce + batch; full rescan if queue > threshold |
| Embedding provider offline | Semantic search disabled, other modes still work |
| Import path unresolvable | Skip edge, continue parsing |

---

## Testing Strategy

| Module | Test type | What is tested |
|---|---|---|
| `store.rs` | Unit | Insert/query nodes, size/date filters, duplicate detection |
| `indexer.rs` | Integration | Scan temp dir with known tree, verify node/edge counts |
| `query.rs` | Unit | Each StructuredQuery variant returns expected results |
| `parser.rs` | Unit | Per-language import extraction with known snippets |
| `llm.rs` | Unit (mocked) | JSON → StructuredQuery parsing; no real API calls in CI |
| `embedder.rs` | Unit (mocked) | Embedding pipeline, HNSW insert/query |
| `community.rs` | Unit | Leiden output on toy graph, summary prompt formatting |

---

## Phase Delivery Order

| Phase | Depends on | Ships when |
|---|---|---|
| 1 — Metadata graph | nothing | First |
| 2 — Content FTS | Phase 1 (nodes exist) | Second |
| 3 — Code dependencies | Phase 2 (symbols extracted) | Third |
| 4 — Semantic search | Phase 2 (content), LLM config | Fourth |
| 5 — GraphRAG | Phase 2 + 4 (content + entities) | Fifth |
