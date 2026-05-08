# Contributing to Tracer

## Prerequisites

- [Rust](https://rustup.rs/) stable
- [Node.js](https://nodejs.org/) 18+
- macOS 13+ (Tauri requires macOS to build)
- Xcode Command Line Tools: `xcode-select --install`

## Local Setup

```bash
git clone https://github.com/Suraj-v23/tracer.git
cd tracer
npm install
npm run build:ts
npm run dev
```

## Development Commands

```bash
npm run build:ts   # compile TypeScript once
npm run watch:ts   # watch mode
npm run dev        # Tauri dev server with hot reload
npm test           # run Vitest unit tests
npm run lint       # ESLint
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml
```

## Making Changes

1. Fork the repo
2. Create a branch: `git checkout -b feat/my-feature`
3. Make changes
4. Run before committing:
   ```bash
   npm run lint && npm test
   cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
   cargo fmt --manifest-path src-tauri/Cargo.toml
   ```
5. Open a Pull Request against `main`

## Project Structure

```
tracer/
├── src-tauri/src/lib.rs   # All Rust commands (filesystem, window management)
└── frontend/js/
    ├── main.ts            # Entry point
    ├── api.ts             # Tauri IPC wrappers
    ├── nodes.ts           # Card rendering, layout, expand/collapse
    ├── canvas.ts          # Wire drawing, pan/zoom
    ├── navigation.ts      # History, breadcrumb
    ├── events.ts          # Mouse/keyboard handlers
    ├── state.ts           # Global mutable state
    └── types.ts           # TypeScript interfaces
```

## Commit Style

```
feat: add collapse animation
fix: breadcrumb double slash on root path
refactor: extract wire registry to canvas module
```

## Reporting Bugs

Open an issue with:
- macOS version
- Steps to reproduce
- Expected vs actual behaviour
- Console output if relevant (View → Developer → JavaScript Console in the app)
