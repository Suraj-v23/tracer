# Tracer — Modular Refactor, Performance & Navigation Design

**Date:** 2026-05-07
**Status:** Approved

---

## Problem Statement

Three pain points in the current codebase:

1. **A — Monolith:** `main.ts` is ~700 lines, all concerns mixed. Hard to navigate, hard to extend.
2. **D — Performance:** Large folders (200+ items) freeze on render. SVG wires lag on pan. Rust scan still blocks on some paths.
3. **E — Navigation:** No forward history. Revisiting a path re-shows loading spinner. No keyboard navigation between nodes.

---

## Approach

Incremental refactor + targeted fixes. No full rewrite. Ship in one implementation cycle.

---

## Section 1: Module Architecture

Split `static/js/main.ts` into feature+layer hybrid modules. Each file owns one concern, imports shared data from `state.ts`, and backend calls from `api.ts`. No circular dependencies — `main.ts` is the only file that imports all modules.

```
static/js/
├── main.ts          # entry point — imports all modules, calls init()
├── api.ts           # all Tauri invoke() wrappers (get_filesystem, delete_item, get_home_dir)
├── state.ts         # single mutable state object (currentPath, history stacks, activeFilter, etc.)
├── store.ts         # Map<path, FsNode> persistent store — replaces fsCache entirely
├── canvas.ts        # pan/zoom transform state + Canvas-based wire drawing
├── nodes.ts         # DOM node creation, column layout, staggered fade-in
├── navigation.ts    # back/forward stacks, navigate(), back(), forward(), keyboard nav
├── sidebar.ts       # sidebar inspector open/close/populate
├── search.ts        # search query handling, filter logic, sort logic, applyFiltersAndRender()
├── events.ts        # all global event bindings (keydown, click-outside, ctx menu, etc.)
└── utils.ts         # format_size, getFileCategory, getColor, TYPE_MAP, TYPE_ICONS, CFG
```

### Dependency Rules

- `utils.ts` — no imports
- `api.ts` — imports nothing from project
- `state.ts` — imports from `utils.ts` only
- `store.ts` — imports `api.ts`, `state.ts`
- `canvas.ts` — imports `state.ts`, `utils.ts`
- `nodes.ts` — imports `state.ts`, `utils.ts`, `canvas.ts`
- `navigation.ts` — imports `state.ts`, `store.ts`, `api.ts`
- `sidebar.ts` — imports `state.ts`, `utils.ts`, `navigation.ts`
- `search.ts` — imports `state.ts`, `nodes.ts`
- `events.ts` — imports all modules (wires DOM events to module functions)
- `main.ts` — imports `events.ts`, `navigation.ts`, `canvas.ts`, `store.ts`

---

## Section 2: Store (replaces fsCache)

Replace the current `fsCache` Map (with TTL, LRU eviction, stale-while-revalidate spread across 5 functions) with a clean persistent store in `store.ts`.

### Data Structure

```ts
const nodes = new Map<string, FsNode>();       // path → scanned node tree
const timestamps = new Map<string, number>();  // path → last fetch time (ms)
const STALE_MS = 120_000;                      // 2 minutes
```

### API

```ts
get(path: string): FsNode | null
set(path: string, node: FsNode): void
isStale(path: string): boolean          // Date.now() - timestamps.get(path) > STALE_MS
invalidate(path: string): void          // removes path + parent
prefetch(paths: string[]): void         // background-fetches top 6 child dirs
revalidate(path: string): Promise<void> // background re-fetch, updates store silently
```

### Behaviour

- `get()` is synchronous and instant — used for navigation, search, render
- Navigation to a stored path renders immediately with no loading spinner
- `isStale()` triggers a background `revalidate()` after render — UI never blocks on it
- `revalidate()` silently re-fetches; if the user is still on that path when it resolves, the scene re-renders with fresh data (no flash — only node sizes / modified times can change)
- `prefetch()` called after every navigation to warm the next likely clicks
- No LRU eviction — filesystem node data is small (< 1MB typical), no need to cap
- Invalidation on delete removes the item and its parent (sizes changed)
- The 30s Rust-side cache remains as a guard against rapid repeated IPC calls

---

## Section 3: Rendering Performance

### 3a — DocumentFragment batch insert

Current code appends each node to `nodesLayer` inside a `forEach` loop, triggering a reflow per node. Fix: accumulate all nodes in a `DocumentFragment`, single `appendChild` at the end.

```ts
const frag = document.createDocumentFragment();
items.forEach((item, i) => frag.appendChild(createDOMNode(item, x, y, query)));
nodesLayer.appendChild(frag); // one reflow
```

### 3b — Idle chunking for large folders

For folders with > 50 items, render the first 50 immediately (synchronous), schedule remaining items in idle chunks via `requestIdleCallback`. User sees content instantly; browser fills in the rest without blocking input.

```
render(items[0..49])  → immediate, via fragment
requestIdleCallback   → render(items[50..99])
requestIdleCallback   → render(items[100..])  ...
```

Each idle chunk also adds the wires for those nodes to the canvas.

### 3c — Canvas wires

Replace the `<svg id="wires-layer">` element with a `<canvas id="wires-layer">`. All bezier curves drawn via `ctx.bezierCurveTo`. On pan/zoom: `ctx.clearRect` + full redraw in one pass. No DOM mutation during pan.

Wire appearance maps 1:1 from current SVG:
- Color: `getColor(item)` — same as border
- Opacity: 0.4 normal, 0.1 dimmed (search active, node not matching)
- Stroke width: 1.5px

Canvas redraws trigger on:
- Pan/zoom (transform change)
- Node drag
- Search query change (dimming changes)
- Navigation (new scene)

---

## Section 4: Navigation

### 4a — Forward stack

Add `forwardStack: string[]` alongside existing `backStack` (rename from `pathHistory`).

```ts
navigate(path):  push currentPath → backStack, clear forwardStack, load path
back():          push currentPath → forwardStack, pop backStack → load
forward():       push currentPath → backStack, pop forwardStack → load
```

Add `←` back and `→` forward buttons to the toolbar (alongside breadcrumb, search, sort). Both buttons show at all times; disabled state (opacity 0.3, not clickable) when the respective stack is empty.

### 4b — Instant revisit

`navigate(path)` checks store first:

```ts
async function navigate(path: string) {
    const cached = store.get(path);
    if (cached) {
        render(cached);                          // instant, synchronous
        if (store.isStale(path)) store.revalidate(path); // background
        return;
    }
    showLoading();
    const node = await api.getFilesystem(path);
    store.set(path, node);
    render(node);
    hideLoading();
}
```

Loading spinner only shows on genuine cache misses.

### 4c — Keyboard navigation

| Key | Action |
|-----|--------|
| `←` `→` `↑` `↓` | Move selection between nodes (column-aware) |
| `Enter` | Open folder or inspect file (same as click) |
| `Backspace` | Go back |
| `Shift+Backspace` or `]` | Go forward |
| `/` | Focus search |
| `Esc` | Deselect / close sidebar / close modals |
| `R` | Reset camera |
| `F5` / `Cmd+R` | Refresh current path |

Selected node tracked in `state.selectedNode`. Arrow navigation computes next node by position in the layout grid (column × row).

---

## Section 5: Color Schema

Shift from neon/saturated to muted, semantic, accessible. One unified palette.

| Type | Old | New |
|------|-----|-----|
| Folder | `#3a7bd5` | `#6b9fd4` |
| Image | `#e85d8a` | `#c47aa0` |
| Video | `#ff7f50` | `#c47d5a` |
| Audio | `#50c878` | `#5a9e7a` |
| Code | `#5cd4ff` | `#5aadd4` |
| Doc | `#ffdd5c` | `#c4a84f` |
| Archive | `#9966ff` | `#8b78c4` |
| Other | `#8892b0` | `#6b7280` |

CSS variable updates:
- `--accent`: `#4f8ef7` → `#6b9fd4` (matches folder, unified)
- `--danger`: `#e05555` → `#c45a5a` (muted red)

Node left-border uses type color. Canvas wires use type color at 40% opacity.

---

## Out of Scope

- WebGL rendering
- Virtual scrolling / viewport culling (idle chunking is sufficient for now)
- Multi-tab / multi-window support
- File preview (images, text)
- Drag-and-drop file operations

---

## Success Criteria

- [ ] `main.ts` is < 50 lines (entry point only)
- [ ] Each module is < 200 lines
- [ ] Folder with 200 items renders without visible freeze
- [ ] Pan over 200 wires has no lag
- [ ] Back + forward navigation works with keyboard and buttons
- [ ] Revisiting any path is instant (no loading spinner)
- [ ] All existing functionality preserved (search, sort, sidebar, delete, context menu)
