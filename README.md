<div align="center">

# Tracer

### A filesystem explorer that thinks like an AI

**Visualise, search, and understand your entire filesystem as an interactive knowledge graph.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021%20Edition-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-v2-purple?style=flat-square&logo=tauri)](https://tauri.app/)
[![Platform](https://img.shields.io/badge/macOS-13%2B-lightgray?style=flat-square&logo=apple)](https://www.apple.com/macos/)
[![TypeScript](https://img.shields.io/badge/TypeScript-Strict-3178c6?style=flat-square&logo=typescript)](https://www.typescriptlang.org/)

![Tracer demo](assets/demo.gif)

</div>

---

## What is Tracer?

Tracer is a native macOS desktop app that renders your filesystem as an **interactive node graph**. Every file and folder appears as a card, connected by bezier wires that reflect real disk usage. It goes beyond a simple visual explorer — Tracer builds a **persistent knowledge graph** of your filesystem, enabling semantic search, AI-powered queries, and automatic community detection across your codebase.

---

## Features

### 🗂️ Visual Graph Explorer
| Feature | Description |
|---|---|
| **Node graph canvas** | Files and folders rendered as wired cards on an infinite, pannable canvas |
| **Size-aware colour coding** | Card colours range from grey → green → yellow → red based on actual APFS block size |
| **Inline expansion** | Right-click a folder → *Open in this space* — children wire directly to the parent node |
| **Multi-window** | Double-click or *Open in new space* launches an independent window at that path |
| **Node dragging** | Drag any card freely; edge-panning activates near viewport boundaries |
| **Collapse branches** | Right-click an expanded folder → *Collapse* removes its children and wires instantly |

### 🔍 Intelligent Search
| Feature | Description |
|---|---|
| **Three search modes** | Switch between **Filter** (live dimming), **Search** (full index query), and **Ask AI** |
| **Metadata search** | Query by name, size, extension, type, or modification date |
| **Full-text search** | Opt-in deep indexing of file contents with BM25 ranking and snippet highlights |
| **Semantic search** | Vector embedding search — find files by *meaning*, not just keywords |
| **AI global queries** | Ask natural-language questions; Tracer answers using GraphRAG across community summaries |

### 🧠 Knowledge Graph (5-Phase System)
| Phase | Capability |
|---|---|
| **1 — Metadata graph** | Background parallel scan with `rayon`; SQLite + petgraph; live file watcher |
| **2 — Content indexing** | Opt-in FTS5 full-text indexing for text and code files |
| **3 — Code dependencies** | Import/reference parsing → dependency edges; forward/reverse BFS; cycle detection |
| **4 — Semantic search** | Local (Ollama) or remote embedding providers; HNSW vector index |
| **5 — GraphRAG** | Leiden community detection; LLM-generated summaries; two-level query answering |

### 📁 File Management
| Feature | Description |
|---|---|
| **Create** | New file or folder via toolbar or right-click menu |
| **Delete** | Confirmation modal before permanent deletion |
| **Move** | Drag-and-drop style move-into-folder via context menu |
| **Accurate sizes** | Physical APFS block accounting (`blocks × 512`) — correct for sparse files and clones |

### 📡 Peer File Transfer
| Feature | Description |
|---|---|
| **LAN discovery** | Finds other Tracer instances on the local network via mDNS |
| **Direct transfer** | Send any file to a nearby device with a one-click code-based handshake |
| **Progress tracking** | Real-time transfer progress bar with background operation |

---

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (stable, 2021 edition+)
- [Node.js](https://nodejs.org/) 18+
- macOS 13 (Ventura) or later
- Xcode Command Line Tools → `xcode-select --install`

### Run in Development

```bash
git clone https://github.com/suraj-v23/tracer.git
cd tracer
npm install
npm run build:ts   # compile TypeScript → frontend/dist/
npm run dev        # launch Tauri dev window with Rust hot-reload
```

### Build a Release `.app`

```bash
npm run build
# → src-tauri/target/release/bundle/macos/Tracer.app
```

---

## Controls

| Action | Input |
|---|---|
| Pan canvas | Click & drag background |
| Zoom in / out | Scroll wheel |
| Drag a node | Click & drag card |
| Expand folder inline | Right-click → *Open in this space* |
| Open in new window | Double-click folder, or right-click → *Open in new space* |
| Collapse branch | Right-click expanded folder → *Collapse* |
| Navigate backward | `Backspace` or ← button |
| Navigate forward | `]` or → button |
| Focus search | `/` |
| Reset canvas | `R` |
| Close all panels | `Esc` |
| Move between nodes | `←` `→` `↑` `↓` |
| Force refresh | `⌘R` or `F5` |

---

## Project Structure

```
tracer/
├── frontend/
│   ├── src/                      # TypeScript source (compiled → dist/)
│   │   ├── api/                  # Tauri IPC wrappers
│   │   │   ├── api.ts            #   Filesystem + transfer commands
│   │   │   └── graph.ts          #   Knowledge graph commands
│   │   ├── components/           # UI and DOM logic
│   │   │   ├── canvas.ts         #   HTML5 Canvas wire drawing + transform
│   │   │   ├── events.ts         #   Global mouse / keyboard event handlers
│   │   │   ├── graphui.ts        #   Search mode UI, results panel, communities list
│   │   │   ├── navigation.ts     #   History stacks, breadcrumb, keyboard nav
│   │   │   ├── nodes.ts          #   Card rendering, layout, drag, expand/collapse
│   │   │   ├── search.ts         #   Filter, sort, and stats
│   │   │   ├── sidebar.ts        #   File details panel
│   │   │   └── transfer.ts       #   Peer file transfer UI
│   │   ├── core/                 # Shared application state
│   │   │   ├── state.ts          #   Global mutable state singleton
│   │   │   ├── store.ts          #   In-memory FS node cache (Map + TTL)
│   │   │   └── types.ts          #   TypeScript interfaces
│   │   ├── utils/                # Pure helper functions
│   │   │   ├── icons.ts          #   Icon constants
│   │   │   └── utils.ts          #   Size formatting, colour mapping, file categories
│   │   └── main.ts               # Application entry point
│   ├── tests/                    # Vitest unit tests (separate from source)
│   ├── styles/
│   │   └── main.css              # Full application stylesheet
│   └── index.html
│
├── src-tauri/src/
│   ├── lib.rs                    # App setup, Tauri commands (FS operations)
│   ├── main.rs                   # Binary entry point
│   ├── graph/                    # Knowledge graph system (5 phases)
│   │   ├── mod.rs                #   GraphAppState + all graph Tauri commands
│   │   ├── store.rs              #   SQLite schema, node/edge CRUD
│   │   ├── indexer.rs            #   Parallel scanner (rayon) + file watcher
│   │   ├── query.rs              #   StructuredQuery execution engine
│   │   ├── llm.rs                #   LLM bridge (Ollama / remote API)
│   │   ├── parser.rs             #   Import/reference extractor per language
│   │   ├── embedder.rs           #   Embedding pipeline + HNSW vector index
│   │   ├── content.rs            #   FTS5 full-text content indexer
│   │   └── community.rs          #   Leiden clustering + community summaries
│   └── transfer/                 # Peer file transfer system
│       ├── mod.rs
│       ├── commands.rs           #   Tauri commands for transfer
│       ├── server.rs             #   TCP transfer server
│       └── discovery.rs          #   mDNS peer discovery
│
├── Cargo.toml                    # Workspace manifest
├── package.json
└── tsconfig.json
```

---

## How It Works

1. **Tauri** launches a native macOS window with an embedded WKWebView — no Electron, no overhead.
2. The webview loads `frontend/index.html` from disk directly; there is no HTTP dev server at runtime.
3. Frontend communicates with the Rust backend via `window.__TAURI_INTERNALS__.invoke(...)`.
4. On startup, a **background Rayon scan** indexes the filesystem into SQLite + petgraph, while a `notify` file watcher keeps the index live.
5. **Wire drawing** uses a single dynamically-resized HTML5 Canvas that repositions itself to cover all active wire endpoints.
6. The **knowledge graph** layers metadata → full-text → dependency → vector → community on top of each other, with each phase adding richer query capability.

---

## Development Scripts

```bash
npm run build:ts    # compile TypeScript → frontend/dist/ (one-shot)
npm run watch:ts    # TypeScript watch mode
npm run dev         # Tauri dev window with Rust hot-reload
npm test            # Vitest unit tests
npm run typecheck   # type-check without emitting output
```

---

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feat/my-feature`
3. Run `cargo clippy && cargo fmt` before committing
4. Ensure `npm test` passes
5. Open a Pull Request with a clear description of your changes

---

## License

[MIT](LICENSE) — © 2026 suraj-v23
