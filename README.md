# Tracer — Filesystem Graph Explorer

> A macOS desktop app that visualises your filesystem as an interactive node graph. Directories and files appear as cards connected by wires — you can expand any folder inline, drag nodes freely, open branches in separate windows, and manage files without leaving the view.

![Tracer demo](assets/demo.gif)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-v2-purple?logo=tauri)](https://tauri.app/)
[![macOS](https://img.shields.io/badge/macOS-13%2B-lightgray?logo=apple)](https://www.apple.com/macos/)

---

## Features

| Feature | Description |
|---------|-------------|
| **Node graph view** | Files and folders rendered as wired cards on an infinite canvas |
| **Size-based colour coding** | Card headers and wires go red → yellow → green → grey based on actual disk size |
| **Inline expansion** | Right-click a folder → *Open in this space* — children appear wired to parent without leaving the current view |
| **Multi-window** | Double-click or *Open in new space* opens a new window at that path |
| **Collapse** | Right-click an expanded folder → *Collapse* — removes the branch and its wires |
| **Drag nodes** | Drag any card anywhere; edge-panning kicks in near viewport boundaries |
| **Create / delete / move** | Toolbar and right-click menu for new file, new folder, move into folder, delete |
| **Navigation** | Back / forward buttons, breadcrumb, Backspace / `]` keyboard shortcuts |
| **Arrow key nav** | ← → ↑ ↓ moves selection between nodes |
| **Search & sort** | `/` to search (dims non-matching); sort by size, name, or type |
| **File details sidebar** | Click any node to see size, path, dates, readonly status |
| **Accurate disk sizes** | Physical APFS block accounting (`blocks × 512`) — correct for sparse files and clones |
| **Fast scanning** | Rayon parallel scan; idle-chunked DOM rendering for large directories |

---

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (stable, 2021 edition or later)
- [Node.js](https://nodejs.org/) 18+ (for the TypeScript compiler)
- macOS 13 (Ventura) or later
- Xcode Command Line Tools: `xcode-select --install`

### Run in Development

```bash
git clone https://github.com/suraj-v23/tracer.git
cd tracer
npm install
npm run build:ts
npm run dev
```

### Build a Release `.app`

```bash
npm run build
# → src-tauri/target/release/bundle/macos/Tracer.app
```

---

## Controls

| Action | Input |
|--------|-------|
| Pan canvas | Click & drag background |
| Zoom | Scroll wheel |
| Drag a node | Click & drag card |
| Expand folder inline | Right-click → *Open in this space* |
| Open in new window | Double-click folder or right-click → *Open in new space* |
| Collapse branch | Right-click expanded folder → *Collapse* |
| Back / undo expansion | `Backspace` or ← button |
| Forward | `]` or → button |
| Focus search | `/` |
| Reset view | `R` |
| Close panels | `Esc` |
| Keyboard navigate | `←` `→` `↑` `↓` |

---

## Architecture

```
tracer/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs           # Tauri entry point
│   │   └── lib.rs            # All Rust commands + FS cache
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── icons/
├── frontend/
│   ├── index.html
│   ├── css/style.css
│   └── js/
│       ├── main.ts           # Entry point — wires modules together
│       ├── api.ts            # Tauri IPC wrappers
│       ├── state.ts          # Global mutable state
│       ├── store.ts          # In-memory FS cache (Map + TTL)
│       ├── navigation.ts     # History stacks, breadcrumb, keyboard nav
│       ├── canvas.ts         # HTML5 Canvas wire drawing + pan/zoom
│       ├── nodes.ts          # Card rendering, layout, drag, expand/collapse
│       ├── search.ts         # Search, sort, filter, stats
│       ├── sidebar.ts        # File details panel
│       ├── events.ts         # Global mouse/keyboard event handlers
│       ├── utils.ts          # Size formatting, colour mapping, file categories
│       └── types.ts          # TypeScript interfaces
├── Cargo.toml                # Workspace manifest
├── package.json
└── tsconfig.json
```

### How it works

1. **Tauri** launches a native macOS window with an embedded WKWebView
2. The webview loads `frontend/index.html` from disk — no HTTP server
3. The frontend invokes Rust commands via `window.__TAURI_INTERNALS__.invoke(...)`:
   - `get_filesystem(path, depth)` — parallel Rayon scan, 30-second TTL cache
   - `create_file` / `create_folder` / `delete_item` / `move_item`
   - `open_in_new_window(path)` — spawns a new Tauri WebviewWindow
4. Wire drawing uses a single **HTML5 Canvas** element that dynamically resizes/repositions to cover all wire endpoints

---

## Development

```bash
npm run build:ts   # compile TypeScript → JS (one-shot)
npm run watch:ts   # watch mode
npm run dev        # Tauri dev mode (Rust hot-reload)
npm test           # Vitest unit tests
```

---

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feat/my-feature`
3. Run `cargo clippy && cargo fmt` before committing
4. Open a Pull Request

---

## License

MIT — see [LICENSE](LICENSE).
