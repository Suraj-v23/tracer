# Tracer — Modular Refactor, Performance & Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split `main.ts` monolith into 10 focused ES modules, replace `fsCache` with a `Map`-based store, switch SVG wires to HTML5 Canvas, add idle-chunked rendering for large folders, implement forward/back navigation with keyboard support, and apply a muted color palette.

**Architecture:** Feature+layer hybrid. `types.ts` and `utils.ts` are pure. `store.ts` wraps `Map<string,FsNode>`. `canvas.ts` owns one `<canvas>` element for all wire drawing. `navigation.ts` manages history stacks and fetches via `store.ts`. Callbacks injected in `main.ts` break potential circular deps. No module imports from `main.ts`.

**Tech Stack:** TypeScript ES modules (native, no bundler), HTML5 Canvas API, `requestIdleCallback` for chunked rendering, Vitest for pure logic tests, Tauri 2 IPC.

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `static/js/types.ts` | Create | `FsNode` interface |
| `static/js/utils.ts` | Create | TYPE_MAP, COLORS, formatSize, getFileCategory, getColor |
| `static/js/api.ts` | Create | Tauri IPC wrappers |
| `static/js/state.ts` | Create | Single mutable global state object |
| `static/js/store.ts` | Create | `Map<string,FsNode>` store, prefetch, revalidate |
| `static/js/canvas.ts` | Create | Canvas wire drawing, pan/zoom transform helpers |
| `static/js/nodes.ts` | Create | DOM node creation, column layout, idle chunking |
| `static/js/navigation.ts` | Create | Back/forward stacks, navigate(), breadcrumb update |
| `static/js/sidebar.ts` | Create | Sidebar panel open/close/populate |
| `static/js/search.ts` | Create | Filter/sort/search, applyFiltersAndRender() |
| `static/js/events.ts` | Create | All global event bindings |
| `static/js/main.ts` | Replace | Entry point only: init(), wire callbacks |
| `static/js/main.js` | Delete | Replaced by compiled output from all modules |
| `static/index.html` | Modify | `type=module`, `<canvas>` wires layer, forward button |
| `static/css/style.css` | Modify | Muted palette vars, node-enter animation, forward btn |
| `tsconfig.json` | Modify | ESNext modules, include all `static/js/**/*.ts` |
| `package.json` | Modify | Add vitest dev dep, `test` script |
| `vitest.config.ts` | Create | Vitest config pointing at static/js |

---

### Task 1: Update tsconfig + delete old main.js

Switch TypeScript to emit native ES modules so each `.ts` file compiles to its own importable `.js`.

**Files:**
- Modify: `tsconfig.json`
- Delete: `static/js/main.js`

- [ ] **Step 1: Replace tsconfig.json contents**

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ES2020",
    "moduleResolution": "bundler",
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "strict": false,
    "noImplicitAny": false,
    "strictNullChecks": false,
    "skipLibCheck": true,
    "noEmitOnError": false,
    "allowJs": false,
    "outDir": "./static/js",
    "rootDir": "./static/js"
  },
  "include": ["static/js/**/*.ts"],
  "exclude": ["static/js/**/*.test.ts", "node_modules"]
}
```

- [ ] **Step 2: Delete old main.js**

```bash
rm /Users/suraj/Documents/tracer/static/js/main.js
```

- [ ] **Step 3: Commit**

```bash
git add tsconfig.json
git rm static/js/main.js
git commit -m "chore: switch tsconfig to ES2020 modules, remove old main.js"
```

---

### Task 2: Set up Vitest

Install Vitest for testing pure logic modules (utils, store, navigation stacks).

**Files:**
- Modify: `package.json`
- Create: `vitest.config.ts`

- [ ] **Step 1: Install vitest**

```bash
npm install -D vitest
```

- [ ] **Step 2: Add test script to package.json**

In `package.json`, add to `"scripts"`:
```json
"test": "vitest run",
"test:watch": "vitest"
```

- [ ] **Step 3: Create vitest.config.ts**

```typescript
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['static/js/**/*.test.ts'],
  },
});
```

- [ ] **Step 4: Verify vitest runs (no tests yet)**

```bash
npm test
```

Expected output: `No test files found`

- [ ] **Step 5: Commit**

```bash
git add package.json package-lock.json vitest.config.ts
git commit -m "chore: add vitest test harness"
```

---

### Task 3: types.ts

Single source of truth for the `FsNode` type (mirrors Rust `FsNode` struct serialization).

**Files:**
- Create: `static/js/types.ts`

- [ ] **Step 1: Create static/js/types.ts**

```typescript
export interface FsNode {
    name: string;
    path: string;
    type: 'directory' | 'file';
    size: number;
    size_human: string;
    modified_time: string;
    readonly: boolean;
    extension?: string;
    children?: FsNode[];
}
```

- [ ] **Step 2: Compile**

```bash
npm run build:ts
```

Expected: `static/js/types.js` emitted, no errors.

- [ ] **Step 3: Commit**

```bash
git add static/js/types.ts static/js/types.js
git commit -m "feat: add FsNode types module"
```

---

### Task 4: utils.ts

Pure helpers — new muted color palette, file category mapping, size formatter.

**Files:**
- Create: `static/js/utils.ts`
- Create: `static/js/utils.test.ts`

- [ ] **Step 1: Write failing tests**

Create `static/js/utils.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { formatSize, getFileCategory, getColor, COLORS } from './utils.js';

describe('formatSize', () => {
    it('returns "0 B" for zero', () => {
        expect(formatSize(0)).toBe('0 B');
    });
    it('formats bytes', () => {
        expect(formatSize(512)).toBe('512 B');
    });
    it('formats kilobytes', () => {
        expect(formatSize(1500)).toBe('1.5 KB');
    });
    it('formats megabytes', () => {
        expect(formatSize(2_500_000)).toBe('2.5 MB');
    });
});

describe('getFileCategory', () => {
    it('returns directory for dir type', () => {
        expect(getFileCategory({ type: 'directory' })).toBe('directory');
    });
    it('identifies image by extension', () => {
        expect(getFileCategory({ type: 'file', extension: '.jpg' })).toBe('image');
    });
    it('identifies code by extension', () => {
        expect(getFileCategory({ type: 'file', extension: '.ts' })).toBe('code');
    });
    it('returns other for unknown extension', () => {
        expect(getFileCategory({ type: 'file', extension: '.xyz' })).toBe('other');
    });
    it('handles missing extension', () => {
        expect(getFileCategory({ type: 'file' })).toBe('other');
    });
});

describe('getColor', () => {
    it('returns folder color for directories', () => {
        expect(getColor({ type: 'directory' })).toBe(COLORS.folder);
    });
    it('returns image color for .png', () => {
        expect(getColor({ type: 'file', extension: '.png' })).toBe(COLORS.image);
    });
    it('returns other color for unknown type', () => {
        expect(getColor({ type: 'file', extension: '.xyz' })).toBe(COLORS.other);
    });
});
```

- [ ] **Step 2: Run tests — verify they fail**

```bash
npm test
```

Expected: `Cannot find module './utils.js'`

- [ ] **Step 3: Create static/js/utils.ts**

```typescript
export const TYPE_MAP: Record<string, string[]> = {
    image:   ['.jpg','.jpeg','.png','.gif','.bmp','.svg','.webp','.ico','.heic'],
    video:   ['.mp4','.avi','.mkv','.mov','.wmv','.flv','.webm'],
    audio:   ['.mp3','.wav','.flac','.aac','.ogg','.wma','.m4a'],
    code:    ['.js','.ts','.py','.java','.cpp','.c','.h','.go','.rs','.rb','.php',
              '.swift','.kt','.sh','.bash','.json','.yaml','.toml','.xml','.css','.html'],
    doc:     ['.pdf','.doc','.docx','.txt','.md','.rtf','.odt','.xls','.xlsx','.ppt','.pptx'],
    archive: ['.zip','.rar','.7z','.tar','.gz','.bz2','.xz','.iso'],
};

export const TYPE_ICONS: Record<string, string> = {
    directory: '📁', image: '🖼', video: '🎬', audio: '🎵',
    code: '💻', doc: '📄', archive: '📦', other: '📎',
};

export const COLORS: Record<string, string> = {
    folder:  '#6b9fd4',
    image:   '#c47aa0',
    video:   '#c47d5a',
    audio:   '#5a9e7a',
    code:    '#5aadd4',
    doc:     '#c4a84f',
    archive: '#8b78c4',
    other:   '#6b7280',
};

export function formatSize(bytes: number): string {
    if (bytes === 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
    let size = bytes;
    let idx = 0;
    while (size >= 1000 && idx < units.length - 1) { size /= 1000; idx++; }
    return idx === 0 ? `${bytes} B` : `${size.toFixed(1)} ${units[idx]}`;
}

export function getFileCategory(item: { type: string; extension?: string }): string {
    if (item.type === 'directory') return 'directory';
    const ext = (item.extension ?? '').toLowerCase();
    for (const [cat, exts] of Object.entries(TYPE_MAP)) {
        if (exts.includes(ext)) return cat;
    }
    return 'other';
}

export function getColor(item: { type: string; extension?: string }): string {
    const cat = getFileCategory(item);
    return cat === 'directory' ? COLORS.folder : (COLORS[cat] ?? COLORS.other);
}
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
npm test
```

Expected: `7 tests passed`

- [ ] **Step 5: Compile**

```bash
npm run build:ts
```

- [ ] **Step 6: Commit**

```bash
git add static/js/utils.ts static/js/utils.js static/js/utils.test.ts
git commit -m "feat: add utils module with muted color palette"
```

---

### Task 5: api.ts

Wraps all three Tauri IPC calls. Shows a helpful error if run outside Tauri.

**Files:**
- Create: `static/js/api.ts`

- [ ] **Step 1: Create static/js/api.ts**

```typescript
import type { FsNode } from './types.js';

function _invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
    const tauri = (window as any).__TAURI_INTERNALS__;
    if (!tauri) {
        const msg = document.getElementById('loading');
        if (msg) {
            msg.innerHTML = '<div style="padding:40px;text-align:center;color:#fff">' +
                '<div style="font-size:2rem;margin-bottom:16px">⚠️</div>' +
                '<div>Run inside Tauri: <code>npm run dev</code></div></div>';
            msg.classList.remove('hidden');
        }
        throw new Error('Tauri runtime not available — open via npm run dev');
    }
    return tauri.invoke(cmd, args);
}

export async function getFilesystem(path: string, depth = 2, force = false): Promise<FsNode> {
    return _invoke('get_filesystem', { path, depth, force }) as Promise<FsNode>;
}

export async function deleteItem(path: string): Promise<void> {
    return _invoke('delete_item', { path }) as Promise<void>;
}

export async function getHomeDir(): Promise<string> {
    return _invoke('get_home_dir') as Promise<string>;
}
```

- [ ] **Step 2: Compile**

```bash
npm run build:ts
```

- [ ] **Step 3: Commit**

```bash
git add static/js/api.ts static/js/api.js
git commit -m "feat: add api module for Tauri IPC"
```

---

### Task 6: state.ts

Single mutable object shared across all modules. Avoids scattered module-level `let` variables.

**Files:**
- Create: `static/js/state.ts`

- [ ] **Step 1: Create static/js/state.ts**

```typescript
import type { FsNode } from './types.js';

export const state = {
    // Navigation
    currentPath:  '',
    currentData:  null as FsNode | null,
    backStack:    [] as string[],
    forwardStack: [] as string[],

    // Filters
    activeFilter: 'all',
    searchQuery:  '',
    sortMode:     'size-desc',

    // Context menu target
    ctxTarget: null as FsNode | null,

    // Selected node element
    selectedNode: null as HTMLElement | null,

    // Canvas pan/zoom
    transform: { x: 100, y: 0, scale: 1 },

    // Canvas pan drag state
    isDragging:      false,
    startDrag:       { x: 0, y: 0 },

    // Node drag state
    draggingNode:    null as HTMLElement | null,
    nodeDragOffset:  { x: 0, y: 0 },
    nodeHasDragged:  false,
};
```

- [ ] **Step 2: Compile**

```bash
npm run build:ts
```

- [ ] **Step 3: Commit**

```bash
git add static/js/state.ts static/js/state.js
git commit -m "feat: add global state module"
```

---

### Task 7: store.ts

Replaces `fsCache`. `Map<string,FsNode>` + timestamps. Synchronous `get()`, background `revalidate()`.

**Files:**
- Create: `static/js/store.ts`
- Create: `static/js/store.test.ts`

- [ ] **Step 1: Write failing tests**

Create `static/js/store.test.ts`:

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock api and state modules before importing store
vi.mock('./api.js', () => ({
    getFilesystem: vi.fn(),
}));
vi.mock('./state.js', () => ({
    state: { currentPath: '/test', searchQuery: '' },
}));

import * as store from './store.js';
import * as api from './api.js';

const mockNode = {
    name: 'test', path: '/test', type: 'directory' as const,
    size: 100, size_human: '100 B', modified_time: '2026-01-01',
    readonly: false, children: [],
};

beforeEach(() => {
    store.invalidate('/test');
    store.invalidate('/parent/child');
    vi.clearAllMocks();
});

describe('get / set', () => {
    it('returns null for unknown path', () => {
        expect(store.get('/nonexistent')).toBeNull();
    });
    it('returns stored node', () => {
        store.set('/test', mockNode);
        expect(store.get('/test')).toBe(mockNode);
    });
});

describe('isStale', () => {
    it('returns true for unknown path', () => {
        expect(store.isStale('/nonexistent')).toBe(true);
    });
    it('returns false immediately after set', () => {
        store.set('/test', mockNode);
        expect(store.isStale('/test')).toBe(false);
    });
});

describe('invalidate', () => {
    it('removes node and parent', () => {
        store.set('/parent/child', mockNode);
        store.set('/parent', mockNode);
        store.invalidate('/parent/child');
        expect(store.get('/parent/child')).toBeNull();
        expect(store.get('/parent')).toBeNull();
    });
});

describe('revalidate', () => {
    it('fetches and updates store', async () => {
        vi.mocked(api.getFilesystem).mockResolvedValue(mockNode);
        await store.revalidate('/test');
        expect(store.get('/test')).toBe(mockNode);
    });
    it('calls onUpdate when on current path', async () => {
        vi.mocked(api.getFilesystem).mockResolvedValue(mockNode);
        const onUpdate = vi.fn();
        await store.revalidate('/test', onUpdate);
        expect(onUpdate).toHaveBeenCalledWith(mockNode);
    });
    it('does not call onUpdate when on different path', async () => {
        vi.mocked(api.getFilesystem).mockResolvedValue(mockNode);
        const onUpdate = vi.fn();
        await store.revalidate('/other', onUpdate);
        expect(onUpdate).not.toHaveBeenCalled();
    });
    it('silently ignores fetch errors', async () => {
        vi.mocked(api.getFilesystem).mockRejectedValue(new Error('network'));
        await expect(store.revalidate('/test')).resolves.toBeUndefined();
    });
});
```

- [ ] **Step 2: Run tests — verify they fail**

```bash
npm test
```

Expected: `Cannot find module './store.js'`

- [ ] **Step 3: Create static/js/store.ts**

```typescript
import type { FsNode } from './types.js';
import * as api from './api.js';
import { state } from './state.js';

const nodes      = new Map<string, FsNode>();
const timestamps = new Map<string, number>();
const STALE_MS   = 120_000; // 2 minutes

export function get(path: string): FsNode | null {
    return nodes.get(path) ?? null;
}

export function set(path: string, node: FsNode): void {
    nodes.set(path, node);
    timestamps.set(path, Date.now());
}

export function isStale(path: string): boolean {
    const ts = timestamps.get(path);
    return ts === undefined || Date.now() - ts > STALE_MS;
}

export function invalidate(path: string): void {
    nodes.delete(path);
    timestamps.delete(path);
    const parent = path.split('/').slice(0, -1).join('/');
    if (parent) {
        nodes.delete(parent);
        timestamps.delete(parent);
    }
}

export async function revalidate(
    path: string,
    onUpdate?: (node: FsNode) => void
): Promise<void> {
    try {
        const node = await api.getFilesystem(path, 2, true);
        set(path, node);
        if (path === state.currentPath) onUpdate?.(node);
    } catch (_) {}
}

export async function prefetch(paths: string[]): Promise<void> {
    const missing = paths.filter(p => !get(p)).slice(0, 6);
    for (const p of missing) {
        await new Promise(r => setTimeout(r, 150));
        try { set(p, await api.getFilesystem(p)); } catch (_) {}
    }
}
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
npm test
```

Expected: `12 tests passed`

- [ ] **Step 5: Compile**

```bash
npm run build:ts
```

- [ ] **Step 6: Commit**

```bash
git add static/js/store.ts static/js/store.js static/js/store.test.ts
git commit -m "feat: add Map-based store replacing fsCache"
```

---

### Task 8: CSS — color palette + node-enter animation

Update CSS variables to muted palette. Add `node-enter` animation (replaces 200 `setTimeout` calls).

**Files:**
- Modify: `static/css/style.css`

- [ ] **Step 1: Update color variables in :root**

Find the `:root` block in `static/css/style.css` and replace it:

```css
:root {
    --bg:         #0a0a0b;
    --bg-panel:   #111113;
    --bg-hover:   #1a1a1e;
    --border:     rgba(255,255,255,0.06);
    --border-hi:  rgba(255,255,255,0.14);
    --accent:     #6b9fd4;
    --danger:     #c45a5a;
    --text:       rgba(255,255,255,0.88);
    --text-dim:   rgba(255,255,255,0.48);
    --text-muted: rgba(255,255,255,0.24);
    --radius:     6px;
}
```

- [ ] **Step 2: Add node-enter animation at end of CSS file (before `.hidden`)**

```css
/* ── Node enter animation ──────────────────────────── */
@keyframes nodeEnter {
    from { opacity: 0; transform: scale(0.85); }
    to   { opacity: 1; transform: scale(1); }
}

.node-enter {
    animation: nodeEnter 0.25s ease forwards;
}
```

- [ ] **Step 3: Remove opacity/transform initial state from .html-node**

In `.html-node` rule, remove any `opacity: 0` or `transform: scale(0.8)` defaults — animation handles this now.

- [ ] **Step 4: Commit**

```bash
git add static/css/style.css
git commit -m "feat: muted color palette + node-enter CSS animation"
```

---

### Task 9: canvas.ts

Owns the `<canvas>` wire layer. Stores wire metadata as `{fromEl, toEl, color, dimmed}`. `redrawWires()` reads current node positions from DOM so drag updates are free.

**Files:**
- Create: `static/js/canvas.ts`

- [ ] **Step 1: Create static/js/canvas.ts**

```typescript
import { state } from './state.js';

interface Wire {
    fromEl: HTMLElement;
    toEl:   HTMLElement;
    color:  string;
    dimmed: boolean;
}

const wireRegistry: Wire[] = [];

function wiresCanvas(): HTMLCanvasElement {
    return document.getElementById('wires-layer') as HTMLCanvasElement;
}

export function getWorkspace(): HTMLElement {
    return document.getElementById('node-workspace') as HTMLElement;
}

export function updateTransform(): void {
    const { x, y, scale } = state.transform;
    getWorkspace().style.transform = `translate(${x}px, ${y}px) scale(${scale})`;
}

export function centerWorkspace(): void {
    state.transform.x     = 100;
    state.transform.y     = window.innerHeight / 2 - 200;
    state.transform.scale = 1;
    updateTransform();
}

export function resizeCanvas(width: number, height: number): void {
    const c  = wiresCanvas();
    c.width  = Math.max(width,  window.innerWidth);
    c.height = Math.max(height, window.innerHeight);
}

export function clearWires(): void {
    wireRegistry.length = 0;
    const c   = wiresCanvas();
    const ctx = c.getContext('2d')!;
    ctx.clearRect(0, 0, c.width, c.height);
}

export function registerWire(
    fromEl: HTMLElement,
    toEl:   HTMLElement,
    color:  string,
    dimmed: boolean
): void {
    wireRegistry.push({ fromEl, toEl, color, dimmed });
}

export function updateWireDimming(query: string): void {
    for (const w of wireRegistry) {
        const name = (w.toEl as any)._itemName as string ?? '';
        w.dimmed = !!query && !name.toLowerCase().includes(query.toLowerCase());
    }
}

export function redrawWires(): void {
    const c   = wiresCanvas();
    const ctx = c.getContext('2d')!;
    ctx.clearRect(0, 0, c.width, c.height);

    const nodeWidth = 200;

    for (const w of wireRegistry) {
        const x1 = parseFloat(w.fromEl.style.left) + nodeWidth;
        const y1 = parseFloat(w.fromEl.style.top)  + 40;
        const x2 = parseFloat(w.toEl.style.left);
        const y2 = parseFloat(w.toEl.style.top)    + 40;

        const dist = Math.abs(x2 - x1) * 0.5;
        ctx.beginPath();
        ctx.moveTo(x1, y1);
        ctx.bezierCurveTo(x1 + dist, y1, x2 - dist, y2, x2, y2);
        ctx.strokeStyle  = w.color;
        ctx.globalAlpha  = w.dimmed ? 0.1 : 0.4;
        ctx.lineWidth    = 1.5;
        ctx.stroke();
    }
    ctx.globalAlpha = 1;
}
```

- [ ] **Step 2: Compile**

```bash
npm run build:ts
```

- [ ] **Step 3: Commit**

```bash
git add static/js/canvas.ts static/js/canvas.js
git commit -m "feat: add canvas module for wire drawing"
```

---

### Task 10: nodes.ts

Creates DOM node cards. Uses `DocumentFragment` for batch insert. First 50 rendered synchronously; rest via `requestIdleCallback` in chunks.

**Files:**
- Create: `static/js/nodes.ts`

- [ ] **Step 1: Create static/js/nodes.ts**

```typescript
import type { FsNode } from './types.js';
import { state } from './state.js';
import { getColor, getFileCategory, TYPE_ICONS } from './utils.js';
import { registerWire, redrawWires, resizeCanvas, clearWires } from './canvas.js';

const COL_HEIGHT  = 8;
const COL_WIDTH   = 320;
const ROW_HEIGHT  = 110;
export const ROOT_X = 100;
export const ROOT_Y = 200;
const NODE_WIDTH  = 200;
const CHUNK_SIZE  = 50;

// Keeps reference to root element for wire registration during idle chunks
let _rootEl: HTMLElement | null = null;
let _renderedItems: FsNode[] = [];

export function getNodesLayer(): HTMLElement {
    return document.getElementById('nodes-layer') as HTMLElement;
}

function getNodePosition(i: number, totalItems: number): { x: number; y: number } {
    const col      = Math.floor(i / COL_HEIGHT);
    const row      = i % COL_HEIGHT;
    const colItems = Math.min(totalItems - col * COL_HEIGHT, COL_HEIGHT);
    return {
        x: ROOT_X + 400 + col * COL_WIDTH,
        y: ROOT_Y + (row - colItems / 2 + 0.5) * ROW_HEIGHT,
    };
}

export function renderScene(
    items: FsNode[],
    rootData: FsNode,
    query: string,
    onNodeClick: (item: FsNode, el: HTMLElement, isRoot: boolean) => void,
    onNodeContextMenu: (item: FsNode, e: MouseEvent) => void
): void {
    const nodesLayer = getNodesLayer();
    nodesLayer.innerHTML = '';
    clearWires();

    _renderedItems = items;

    // Size canvas to cover all nodes
    const numCols = Math.ceil(items.length / COL_HEIGHT) || 1;
    resizeCanvas(
        ROOT_X + 400 + numCols * COL_WIDTH + NODE_WIDTH + 50,
        ROOT_Y + COL_HEIGHT * ROW_HEIGHT + 100
    );

    // Root node
    _rootEl = createNodeEl(rootData, ROOT_X, ROOT_Y, null, true, onNodeClick, onNodeContextMenu);
    nodesLayer.appendChild(_rootEl);

    if (!items.length) return;

    // First chunk — synchronous, user sees content immediately
    appendChunk(items, 0, Math.min(CHUNK_SIZE, items.length), query, nodesLayer, onNodeClick, onNodeContextMenu);
    redrawWires();

    // Remaining chunks — deferred to idle time
    if (items.length > CHUNK_SIZE) {
        scheduleChunk(items, CHUNK_SIZE, query, nodesLayer, onNodeClick, onNodeContextMenu);
    }
}

function appendChunk(
    items: FsNode[],
    start: number,
    end: number,
    query: string,
    nodesLayer: HTMLElement,
    onNodeClick: (item: FsNode, el: HTMLElement, isRoot: boolean) => void,
    onNodeContextMenu: (item: FsNode, e: MouseEvent) => void
): void {
    const frag = document.createDocumentFragment();
    for (let i = start; i < end; i++) {
        const item   = items[i];
        const { x, y } = getNodePosition(i, items.length);
        const dimmed = !!query && !item.name.toLowerCase().includes(query.toLowerCase());
        const el     = createNodeEl(item, x, y, query, false, onNodeClick, onNodeContextMenu);
        el.style.animationDelay = `${(i - start) * 15}ms`;
        registerWire(_rootEl!, el, getColor(item), dimmed);
        frag.appendChild(el);
    }
    nodesLayer.appendChild(frag);
}

function scheduleChunk(
    items: FsNode[],
    startIdx: number,
    query: string,
    nodesLayer: HTMLElement,
    onNodeClick: (item: FsNode, el: HTMLElement, isRoot: boolean) => void,
    onNodeContextMenu: (item: FsNode, e: MouseEvent) => void
): void {
    requestIdleCallback(() => {
        const end = Math.min(startIdx + CHUNK_SIZE, items.length);
        appendChunk(items, startIdx, end, query, nodesLayer, onNodeClick, onNodeContextMenu);
        redrawWires();
        if (end < items.length) {
            scheduleChunk(items, end, query, nodesLayer, onNodeClick, onNodeContextMenu);
        }
    });
}

export function createNodeEl(
    item: FsNode,
    x: number,
    y: number,
    query: string | null,
    isRoot: boolean,
    onNodeClick: (item: FsNode, el: HTMLElement, isRoot: boolean) => void,
    onNodeContextMenu: (item: FsNode, e: MouseEvent) => void
): HTMLElement {
    const el       = document.createElement('div');
    el.className   = isRoot ? 'html-node root-node' : 'html-node node-enter';
    const color    = isRoot ? 'var(--accent)' : getColor(item);
    const cat      = getFileCategory(item);
    const icon     = TYPE_ICONS[cat] ?? '📎';
    const dimmed   = !isRoot && !!query && !item.name.toLowerCase().includes(query.toLowerCase());

    if (dimmed) el.classList.add('dimmed');

    (el as any)._itemName = item.name;

    el.style.borderLeftColor = color;
    el.style.left = `${x}px`;
    el.style.top  = `${y}px`;

    el.innerHTML = `
        <div class="node-header" style="border-bottom-color:${color}22">
            <span class="node-icon">${icon}</span>
            <span class="node-title" title="${item.name}">${item.name}</span>
        </div>
        <div class="node-body">
            <div class="node-detail-row">
                <span class="node-detail">${item.type === 'directory' ? 'Folder' : (item.extension ?? 'File')}</span>
                <span class="node-detail size">${item.size_human}</span>
            </div>
            <div class="node-detail-row node-meta">
                <span>${item.modified_time}</span>
                ${item.readonly ? '<span style="color:var(--danger)">Read-only</span>' : ''}
            </div>
        </div>
        <div class="node-port input-port"></div>
        ${isRoot ? '<div class="node-port output-port"></div>' : ''}
    `;

    el.addEventListener('mousedown', (e) => {
        if (e.button !== 0) return;
        e.stopPropagation();
        const mouseX = (e.clientX - state.transform.x) / state.transform.scale;
        const mouseY = (e.clientY - state.transform.y) / state.transform.scale;
        state.nodeDragOffset.x = mouseX - parseFloat(el.style.left);
        state.nodeDragOffset.y = mouseY - parseFloat(el.style.top);
        state.draggingNode  = el;
        state.nodeHasDragged = false;
        el.style.transition = 'none';
    });

    el.addEventListener('click', (e) => {
        e.stopPropagation();
        if (state.nodeHasDragged) { state.nodeHasDragged = false; return; }
        if (state.selectedNode) state.selectedNode.classList.remove('selected');
        el.classList.add('selected');
        state.selectedNode = el;
        onNodeClick(item, el, isRoot);
    });

    el.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        e.stopPropagation();
        onNodeContextMenu(item, e);
    });

    return el;
}

export function rerenderDimming(query: string): void {
    const nodesLayer = getNodesLayer();
    nodesLayer.querySelectorAll<HTMLElement>('.html-node:not(.root-node)').forEach(el => {
        const name = (el as any)._itemName as string ?? '';
        const dimmed = !!query && !name.toLowerCase().includes(query.toLowerCase());
        el.classList.toggle('dimmed', dimmed);
    });
}
```

- [ ] **Step 2: Compile**

```bash
npm run build:ts
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add static/js/nodes.ts static/js/nodes.js
git commit -m "feat: add nodes module with DocumentFragment + idle chunking"
```

---

### Task 11: navigation.ts

Back/forward history stacks. `navigate()` checks store first (instant), falls back to IPC fetch. Keyboard arrow navigation between nodes. Breadcrumb update.

**Files:**
- Create: `static/js/navigation.ts`
- Create: `static/js/navigation.test.ts`

- [ ] **Step 1: Write failing tests for stack logic**

Create `static/js/navigation.test.ts`:

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('./api.js',   () => ({ getFilesystem: vi.fn(), getHomeDir: vi.fn() }));
vi.mock('./state.js', () => ({
    state: {
        currentPath: '', currentData: null,
        backStack: [], forwardStack: [],
        searchQuery: '', transform: { x: 0, y: 0, scale: 1 },
        selectedNode: null,
    }
}));
vi.mock('./store.js', () => ({
    get: vi.fn(() => null),
    set: vi.fn(),
    isStale: vi.fn(() => false),
    revalidate: vi.fn(),
    prefetch: vi.fn(),
}));

// Mock DOM
const mockBreadcrumb = { innerHTML: '', querySelectorAll: vi.fn(() => []) };
vi.stubGlobal('document', {
    getElementById: vi.fn((id: string) => {
        if (id === 'breadcrumb') return mockBreadcrumb;
        if (id === 'loading')    return { classList: { remove: vi.fn(), add: vi.fn() }, textContent: '' };
        if (id === 'loading-path') return { textContent: '' };
        return null;
    }),
});

import { state } from './state.js';
import * as store from './store.js';
import { canGoBack, canGoForward, recordNavigate, back, forward } from './navigation.js';

beforeEach(() => {
    state.currentPath  = '';
    state.backStack    = [];
    state.forwardStack = [];
});

describe('canGoBack / canGoForward', () => {
    it('returns false when stacks empty', () => {
        expect(canGoBack()).toBe(false);
        expect(canGoForward()).toBe(false);
    });
    it('canGoBack after recording navigation', () => {
        state.currentPath = '/home';
        recordNavigate('/home/docs');
        expect(canGoBack()).toBe(true);
    });
});

describe('recordNavigate', () => {
    it('pushes currentPath to backStack and clears forwardStack', () => {
        state.currentPath  = '/home';
        state.forwardStack = ['/prev'];
        recordNavigate('/home/docs');
        expect(state.backStack).toContain('/home');
        expect(state.forwardStack).toHaveLength(0);
    });
});

describe('back / forward', () => {
    it('back pops backStack, pushes to forwardStack', () => {
        state.backStack    = ['/home'];
        state.currentPath  = '/home/docs';
        state.forwardStack = [];
        const path = back();
        expect(path).toBe('/home');
        expect(state.forwardStack).toContain('/home/docs');
        expect(state.backStack).toHaveLength(0);
    });
    it('forward pops forwardStack, pushes to backStack', () => {
        state.backStack    = ['/home'];
        state.currentPath  = '/home/docs';
        state.forwardStack = ['/home/docs/sub'];
        const path = forward();
        expect(path).toBe('/home/docs/sub');
        expect(state.backStack).toContain('/home/docs');
    });
    it('back returns null when stack empty', () => {
        expect(back()).toBeNull();
    });
    it('forward returns null when stack empty', () => {
        expect(forward()).toBeNull();
    });
});
```

- [ ] **Step 2: Run tests — verify they fail**

```bash
npm test
```

Expected: `Cannot find module './navigation.js'`

- [ ] **Step 3: Create static/js/navigation.ts**

```typescript
import type { FsNode } from './types.js';
import { state } from './state.js';
import * as store from './store.js';
import * as api from './api.js';

// Callback injected by main.ts — called after state.currentData is updated
let _onNavigate: ((node: FsNode) => void) | null = null;

export function setOnNavigate(fn: (node: FsNode) => void): void {
    _onNavigate = fn;
}

export function canGoBack():    boolean { return state.backStack.length > 0; }
export function canGoForward(): boolean { return state.forwardStack.length > 0; }

export function recordNavigate(newPath: string): void {
    if (state.currentPath) state.backStack.push(state.currentPath);
    state.forwardStack = [];
}

export function back(): string | null {
    if (!canGoBack()) return null;
    const prev = state.backStack.pop()!;
    state.forwardStack.push(state.currentPath);
    return prev;
}

export function forward(): string | null {
    if (!canGoForward()) return null;
    const next = state.forwardStack.pop()!;
    state.backStack.push(state.currentPath);
    return next;
}

export async function navigate(path: string): Promise<void> {
    recordNavigate(path);

    const cached = store.get(path);
    if (cached) {
        _setCurrentAndRender(path, cached);
        if (store.isStale(path)) store.revalidate(path, n => _setCurrentAndRender(path, n));
        return;
    }

    showLoading(path);
    try {
        const node = await api.getFilesystem(path);
        store.set(path, node);
        _setCurrentAndRender(path, node);
    } catch (e) {
        console.error(e);
    } finally {
        hideLoading();
    }
}

function _setCurrentAndRender(path: string, node: FsNode): void {
    state.currentPath = path;
    state.currentData = node;
    updateBreadcrumb(path);
    _onNavigate?.(node);
    // Prefetch top child dirs in background
    const childDirs = (node.children ?? [])
        .filter(c => c.type === 'directory')
        .map(c => c.path);
    store.prefetch(childDirs);
}

function showLoading(path: string): void {
    const el   = document.getElementById('loading');
    const path_ = document.getElementById('loading-path');
    if (el)    el.classList.remove('hidden');
    if (path_) path_.textContent = path;
}

function hideLoading(): void {
    document.getElementById('loading')?.classList.add('hidden');
}

export function updateBreadcrumb(path: string): void {
    const breadcrumb = document.getElementById('breadcrumb');
    if (!breadcrumb) return;

    const parts = path.split('/').filter(Boolean);
    let html = `<span class="crumb" data-path="/">/</span>`;
    let cur = '';
    for (const p of parts) {
        cur += '/' + p;
        html += `<span class="crumb-sep">/</span><span class="crumb" data-path="${cur}">${p}</span>`;
    }
    breadcrumb.innerHTML = html;
    breadcrumb.querySelectorAll<HTMLElement>('.crumb').forEach(el => {
        el.addEventListener('click', e => {
            e.stopPropagation();
            navigate(el.dataset.path!);
        });
    });
}

// Arrow key navigation between visible nodes
export function navigateNodes(key: string): void {
    const nodesLayer = document.getElementById('nodes-layer');
    if (!nodesLayer) return;

    const allNodes = Array.from(
        nodesLayer.querySelectorAll<HTMLElement>('.html-node:not(.root-node):not(.dimmed)')
    );
    if (!allNodes.length) return;

    const currentIdx = state.selectedNode
        ? allNodes.indexOf(state.selectedNode)
        : -1;

    let nextIdx = currentIdx;
    const COL_HEIGHT = 8;

    if (key === 'ArrowRight') nextIdx = currentIdx + COL_HEIGHT;
    if (key === 'ArrowLeft')  nextIdx = currentIdx - COL_HEIGHT;
    if (key === 'ArrowDown')  nextIdx = currentIdx + 1;
    if (key === 'ArrowUp')    nextIdx = currentIdx - 1;

    nextIdx = Math.max(0, Math.min(nextIdx, allNodes.length - 1));
    if (nextIdx === currentIdx && currentIdx !== -1) return;

    if (state.selectedNode) state.selectedNode.classList.remove('selected');
    const next = allNodes[nextIdx === -1 ? 0 : nextIdx];
    next.classList.add('selected');
    state.selectedNode = next;
    next.scrollIntoView?.({ block: 'nearest' });
}
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
npm test
```

Expected: `10 tests passed`

- [ ] **Step 5: Compile**

```bash
npm run build:ts
```

- [ ] **Step 6: Commit**

```bash
git add static/js/navigation.ts static/js/navigation.js static/js/navigation.test.ts
git commit -m "feat: add navigation module with forward/back stacks and arrow key nav"
```

---

### Task 12: sidebar.ts

Sidebar panel open/close/populate. Handles the Delete button modal trigger.

**Files:**
- Create: `static/js/sidebar.ts`

- [ ] **Step 1: Create static/js/sidebar.ts**

```typescript
import type { FsNode } from './types.js';
import { state } from './state.js';
import { getFileCategory, TYPE_ICONS } from './utils.js';
import { navigate } from './navigation.js';

export function openSidebar(item: FsNode): void {
    const isDir = item.type === 'directory';
    const cat   = getFileCategory(item);

    _set('sb-icon',      TYPE_ICONS[cat] ?? '📎');
    _set('sb-name',      item.name);
    _set('sb-size-badge', item.size_human);
    _set('sb-type',      isDir ? 'Folder' : (item.extension ?? 'file').replace('.', '').toUpperCase());
    _set('sb-size',      item.size_human);
    _set('sb-path',      item.path);
    _set('sb-modified',  item.modified_time);
    _set('sb-readonly',  item.readonly ? 'Yes' : 'No');

    const sbEnter = document.getElementById('sb-enter');
    if (sbEnter) {
        if (isDir) {
            sbEnter.classList.remove('hidden');
            sbEnter.onclick = () => navigate(item.path);
        } else {
            sbEnter.classList.add('hidden');
        }
    }

    document.getElementById('sidebar')?.classList.remove('hidden');
}

export function closeSidebar(): void {
    document.getElementById('sidebar')?.classList.add('hidden');
    if (state.selectedNode) {
        state.selectedNode.classList.remove('selected');
        state.selectedNode = null;
    }
}

export function setDeleteHandler(onDelete: (item: FsNode) => void): void {
    // Wired by events.ts — called when Delete button clicked from sidebar
    const btn = document.getElementById('sb-delete');
    if (!btn) return;
    btn.onclick = () => {
        const sidebar = document.getElementById('sidebar');
        const item    = (sidebar as any)._currentItem as FsNode | undefined;
        if (item) onDelete(item);
    };
}

export function setSidebarItem(item: FsNode): void {
    const sidebar = document.getElementById('sidebar');
    if (sidebar) (sidebar as any)._currentItem = item;
}

function _set(id: string, value: string): void {
    const el = document.getElementById(id);
    if (el) el.textContent = value;
}
```

- [ ] **Step 2: Compile**

```bash
npm run build:ts
```

- [ ] **Step 3: Commit**

```bash
git add static/js/sidebar.ts static/js/sidebar.js
git commit -m "feat: add sidebar module"
```

---

### Task 13: search.ts

Filter/sort `state.currentData.children`, call `nodes.renderScene()`. Exposes `applyFiltersAndRender()` used by navigation and event handlers.

**Files:**
- Create: `static/js/search.ts`

- [ ] **Step 1: Create static/js/search.ts**

```typescript
import type { FsNode } from './types.js';
import { state } from './state.js';
import { getFileCategory } from './utils.js';
import { renderScene } from './nodes.js';
import { updateWireDimming, redrawWires } from './canvas.js';
import { rerenderDimming } from './nodes.js';

// Callbacks injected by main.ts
let _onNodeClick:       ((item: FsNode, el: HTMLElement, isRoot: boolean) => void) | null = null;
let _onNodeContextMenu: ((item: FsNode, e: MouseEvent) => void) | null = null;

export function setCallbacks(
    onNodeClick:       (item: FsNode, el: HTMLElement, isRoot: boolean) => void,
    onNodeContextMenu: (item: FsNode, e: MouseEvent) => void
): void {
    _onNodeClick       = onNodeClick;
    _onNodeContextMenu = onNodeContextMenu;
}

export function applyFiltersAndRender(): void {
    if (!state.currentData) return;
    const items = getFilteredSortedItems();
    updateMatchCount(items.length, (state.currentData.children ?? []).length);

    renderScene(
        items,
        state.currentData,
        state.searchQuery,
        _onNodeClick!,
        _onNodeContextMenu!
    );
}

export function applySearch(query: string): void {
    state.searchQuery = query;
    const searchClear = document.getElementById('search-clear');
    if (searchClear) searchClear.classList.toggle('hidden', !query);

    // Fast path: just update dimming without full re-render
    updateWireDimming(query);
    rerenderDimming(query);
    redrawWires();
    updateMatchCount(
        getFilteredSortedItems().length,
        (state.currentData?.children ?? []).length
    );
}

function getFilteredSortedItems(): FsNode[] {
    let items = [...(state.currentData?.children ?? [])];

    // Sort
    items.sort((a, b) => {
        if (state.sortMode === 'size-desc') return b.size - a.size;
        if (state.sortMode === 'size-asc')  return a.size - b.size;
        if (state.sortMode === 'name-asc')  return a.name.localeCompare(b.name);
        if (state.sortMode === 'name-desc') return b.name.localeCompare(a.name);
        if (state.sortMode === 'type')      return getFileCategory(a).localeCompare(getFileCategory(b));
        return 0;
    });

    // Filter by type
    if (state.activeFilter !== 'all') {
        items = items.filter(i =>
            state.activeFilter === 'directory'
                ? i.type === 'directory'
                : getFileCategory(i) === state.activeFilter
        );
    }

    // NOTE: search query is NOT filtered here — it only dims nodes visually.
    // applySearch() handles dimming without a full re-render.

    return items;
}

function updateMatchCount(shown: number, total: number): void {
    const el = document.getElementById('match-count');
    if (!el) return;
    if (state.searchQuery) {
        el.textContent = `${shown} match${shown !== 1 ? 'es' : ''}`;
    } else {
        el.textContent = '';
    }
}

export function updateStats(): void {
    if (!state.currentData) return;
    const items   = state.currentData.children ?? [];
    const folders = items.filter(i => i.type === 'directory').length;
    const files   = items.length - folders;
    const ic = document.getElementById('item-count');
    const ts = document.getElementById('total-size');
    if (ic) ic.textContent = `${items.length} items (${folders} folders, ${files} files)`;
    if (ts) ts.textContent = state.currentData.size_human ?? '0 B';
}
```

- [ ] **Step 2: Compile**

```bash
npm run build:ts
```

- [ ] **Step 3: Commit**

```bash
git add static/js/search.ts static/js/search.js
git commit -m "feat: add search/filter/sort module"
```

---

### Task 14: events.ts

All global DOM event bindings. Wires keyboard, canvas pan/zoom, context menu, confirm modal, sidebar delete.

**Files:**
- Create: `static/js/events.ts`

- [ ] **Step 1: Create static/js/events.ts**

```typescript
import type { FsNode } from './types.js';
import { state } from './state.js';
import * as nav from './navigation.js';
import { applyFiltersAndRender, applySearch, updateStats } from './search.js';
import { openSidebar, closeSidebar, setSidebarItem } from './sidebar.js';
import { centerWorkspace, updateTransform, redrawWires } from './canvas.js';
import * as store from './store.js';
import * as api from './api.js';
// toast defined locally — avoids circular dep with main.ts

export function bindCanvasEvents(): void {
    const container = document.getElementById('canvas-container')!;
    const workspace = document.getElementById('node-workspace')!;

    container.addEventListener('mousedown', e => {
        if (e.button !== 0 && e.button !== 2) return;
        if ((e.target as HTMLElement).closest('.html-node')) return;
        state.isDragging = true;
        state.startDrag  = { x: e.clientX - state.transform.x, y: e.clientY - state.transform.y };
        container.style.cursor = 'grabbing';
    });

    window.addEventListener('mousemove', e => {
        if (state.draggingNode) {
            state.nodeHasDragged = true;
            const mouseX = (e.clientX - state.transform.x) / state.transform.scale;
            const mouseY = (e.clientY - state.transform.y) / state.transform.scale;
            state.draggingNode.style.left = `${mouseX - state.nodeDragOffset.x}px`;
            state.draggingNode.style.top  = `${mouseY - state.nodeDragOffset.y}px`;
            redrawWires();
            return;
        }
        if (!state.isDragging) return;
        state.transform.x = e.clientX - state.startDrag.x;
        state.transform.y = e.clientY - state.startDrag.y;
        updateTransform();
    });

    window.addEventListener('mouseup', () => {
        if (state.draggingNode) {
            state.draggingNode.style.transition = '';
            setTimeout(() => { state.draggingNode = null; }, 0);
        }
        state.isDragging = false;
        container.style.cursor = 'grab';
    });

    container.addEventListener('wheel', e => {
        if ((e.target as HTMLElement).closest('.html-node')) return;
        e.preventDefault();
        const delta    = -e.deltaY * 0.001;
        const newScale = Math.min(Math.max(0.1, state.transform.scale * (1 + delta)), 3);
        const rect     = container.getBoundingClientRect();
        const mouseX   = e.clientX - rect.left;
        const mouseY   = e.clientY - rect.top;
        state.transform.x     = mouseX - (mouseX - state.transform.x) * (newScale / state.transform.scale);
        state.transform.y     = mouseY - (mouseY - state.transform.y) * (newScale / state.transform.scale);
        state.transform.scale = newScale;
        updateTransform();
    }, { passive: false });
}

export function bindGlobalEvents(): void {
    // Click outside — close menus/sidebar
    window.addEventListener('click', e => {
        const target = e.target as HTMLElement;
        const ctxMenu = document.getElementById('ctx-menu')!;
        if (!ctxMenu.contains(target)) ctxMenu.classList.add('hidden');
        if (!document.getElementById('sidebar')!.contains(target)
            && !target.closest('#toolbar')
            && !target.closest('.html-node')) {
            closeSidebar();
        }
    });

    // Context menu actions
    document.getElementById('ctx-open')!.addEventListener('click', () => {
        if (state.ctxTarget?.type === 'directory') nav.navigate(state.ctxTarget.path);
        document.getElementById('ctx-menu')!.classList.add('hidden');
    });

    document.getElementById('ctx-info')!.addEventListener('click', () => {
        if (state.ctxTarget) openSidebar(state.ctxTarget);
        document.getElementById('ctx-menu')!.classList.add('hidden');
    });

    document.getElementById('ctx-delete')!.addEventListener('click', () => {
        if (state.ctxTarget) showDeleteModal(state.ctxTarget);
        document.getElementById('ctx-menu')!.classList.add('hidden');
    });

    document.getElementById('sb-delete')!.addEventListener('click', () => {
        const sidebar = document.getElementById('sidebar')!;
        const item    = (sidebar as any)._currentItem as FsNode | undefined;
        if (item) showDeleteModal(item);
    });

    document.getElementById('sidebar-close')!.addEventListener('click', closeSidebar);

    // Search
    const searchInput = document.getElementById('search-input') as HTMLInputElement;
    searchInput.addEventListener('input', e => applySearch((e.target as HTMLInputElement).value.trim()));

    document.getElementById('search-clear')!.addEventListener('click', () => {
        searchInput.value = '';
        applySearch('');
        searchInput.focus();
    });

    // Sort
    const sortSelect = document.getElementById('sort-select') as HTMLSelectElement;
    sortSelect.addEventListener('change', () => {
        state.sortMode = sortSelect.value;
        applyFiltersAndRender();
    });

    // Keyboard
    window.addEventListener('keydown', e => {
        const searchInput = document.getElementById('search-input') as HTMLInputElement;
        if (document.activeElement === searchInput) {
            if (e.key === 'Escape') { searchInput.blur(); searchInput.value = ''; applySearch(''); }
            return;
        }

        if (['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(e.key)) {
            e.preventDefault();
            nav.navigateNodes(e.key);
            return;
        }

        if (e.key === 'Enter' && state.selectedNode) {
            state.selectedNode.click();
            return;
        }

        if (e.key === 'Backspace' && nav.canGoBack()) {
            e.preventDefault();
            const prev = nav.back();
            if (prev) nav.navigate(prev);
            return;
        }

        if ((e.key === ']' || (e.key === 'Backspace' && e.shiftKey)) && nav.canGoForward()) {
            e.preventDefault();
            const next = nav.forward();
            if (next) nav.navigate(next);
            return;
        }

        if (e.key === 'Escape') {
            closeSidebar();
            document.getElementById('ctx-menu')!.classList.add('hidden');
            document.getElementById('confirm-modal')!.classList.add('hidden');
        }
        if (e.key === 'r' || e.key === 'R')                   centerWorkspace();
        if (e.key === 'F5' || ((e.metaKey || e.ctrlKey) && e.key === 'r')) {
            e.preventDefault();
            store.invalidate(state.currentPath);
            nav.navigate(state.currentPath);
        }
        if (e.key === '/') { e.preventDefault(); searchInput.focus(); }
    });

    // Back / Forward buttons
    const btnBack    = document.getElementById('btn-back') as HTMLButtonElement | null;
    const btnForward = document.getElementById('btn-forward') as HTMLButtonElement | null;

    btnBack?.addEventListener('click', () => {
        const prev = nav.back();
        if (prev) nav.navigate(prev);
    });

    btnForward?.addEventListener('click', () => {
        const next = nav.forward();
        if (next) nav.navigate(next);
    });
}

export function toast(msg: string, type = ''): void {
    const el       = document.createElement('div');
    el.className   = `toast ${type}`;
    el.textContent = msg;
    document.getElementById('toast-container')?.appendChild(el);
    setTimeout(() => {
        el.style.animation = 'toastOut 0.25s forwards';
        setTimeout(() => el.remove(), 250);
    }, 2500);
}

export function showDeleteModal(item: FsNode): void {
    const confirmText  = document.getElementById('confirm-text')!;
    const confirmModal = document.getElementById('confirm-modal')!;
    confirmText.innerHTML = `Delete <strong>${item.name}</strong>?`;
    confirmModal.classList.remove('hidden');

    document.getElementById('btn-delete')!.onclick = async () => {
        try {
            await api.deleteItem(item.path);
            store.invalidate(item.path);
            store.invalidate(state.currentPath);
            toast(`Deleted ${item.name}`, 'success');
            closeSidebar();
            await nav.navigate(state.currentPath);
        } catch (err) {
            toast('Error deleting: ' + err, 'error');
        }
        confirmModal.classList.add('hidden');
    };

    document.getElementById('btn-cancel')!.onclick = () => confirmModal.classList.add('hidden');
}

export function bindNodeContextMenu(item: FsNode, e: MouseEvent): void {
    state.ctxTarget = item;
    const isDir     = item.type === 'directory';
    document.getElementById('ctx-open')!.classList.toggle('disabled', !isDir);
    const ctxMenu = document.getElementById('ctx-menu')!;
    ctxMenu.style.left = Math.min(e.clientX, window.innerWidth - 190) + 'px';
    ctxMenu.style.top  = Math.min(e.clientY, window.innerHeight - 130) + 'px';
    ctxMenu.classList.remove('hidden');
}

export function handleNodeClick(item: FsNode, el: HTMLElement, isRoot: boolean): void {
    setSidebarItem(item);
    if (item.type === 'directory' && !isRoot) {
        nav.navigate(item.path);
    } else {
        openSidebar(item);
    }
}
```

- [ ] **Step 2: Compile**

```bash
npm run build:ts
```

- [ ] **Step 3: Commit**

```bash
git add static/js/events.ts static/js/events.js
git commit -m "feat: add events module with keyboard nav + forward/back handlers"
```

---

### Task 15: main.ts (entry point) + index.html

Slim `main.ts` to < 50 lines. Update HTML: `type=module`, `<canvas>` wires layer, forward button. Export `toast` from main.ts (used by events.ts).

**Files:**
- Replace: `static/js/main.ts`
- Modify: `static/index.html`

- [ ] **Step 1: Replace static/js/main.ts**

```typescript
import * as api from './api.js';
import * as nav from './navigation.js';
import * as search from './search.js';
import { centerWorkspace } from './canvas.js';
import { bindCanvasEvents, bindGlobalEvents, handleNodeClick, bindNodeContextMenu } from './events.js';

async function init(): Promise<void> {
    // Wire navigation → search render
    nav.setOnNavigate(node => {
        search.applyFiltersAndRender();
        search.updateStats();
    });

    // Wire search → node callbacks
    search.setCallbacks(handleNodeClick, bindNodeContextMenu);

    bindCanvasEvents();
    bindGlobalEvents();
    centerWorkspace();

    const homeDir = await api.getHomeDir().catch(() => '/Users');
    await nav.navigate(homeDir);
    document.getElementById('loading')?.classList.add('hidden');
}

init();
```

- [ ] **Step 2: Update static/index.html**

Make three changes:

**a) Change script tag to module and update path:**
```html
<script type="module" src="./js/main.js"></script>
```

**b) Change `<svg id="wires-layer">` to `<canvas>`:**
```html
<canvas id="wires-layer"></canvas>
```

**c) Add forward button and back button to toolbar (after breadcrumb-wrap, before search-wrap):**
```html
<button id="btn-back" class="toolbar-btn" title="Go back (Backspace)" disabled>←</button>
<button id="btn-forward" class="toolbar-btn" title="Go forward (])" disabled>→</button>
```

- [ ] **Step 3: Add wires-layer canvas CSS**

In `static/css/style.css`, update the `#wires-layer` rule:

```css
#wires-layer {
    position: absolute; top: 0; left: 0;
    pointer-events: none;
    display: block;
}
```

- [ ] **Step 4: Compile**

```bash
npm run build:ts
```

Expected: `static/js/main.js` and all module `.js` files emitted, no errors.

- [ ] **Step 5: Run the app and verify**

```bash
npm run dev
```

Verify:
- App loads, home directory renders as node graph
- Node cards use muted colors (no neon)
- Large folders render first 50 nodes instantly, rest fade in
- Wires draw correctly (no SVG, canvas-based)
- Back (←) and Forward (→) buttons work
- `Backspace` goes back, `]` goes forward
- Arrow keys move node selection
- `Enter` opens folder or sidebar
- Search dims non-matching nodes without full re-render
- Delete still works

- [ ] **Step 6: Commit**

```bash
git add static/js/main.ts static/js/main.js static/index.html static/css/style.css
git commit -m "feat: wire all modules in main.ts, update HTML for ESM + canvas wires + nav buttons"
```

---

### Task 16: Run all tests + final verification

- [ ] **Step 1: Run full test suite**

```bash
npm test
```

Expected: all tests in `utils.test.ts`, `store.test.ts`, `navigation.test.ts` pass.

- [ ] **Step 2: Check module sizes**

```bash
wc -l static/js/*.ts | sort -n
```

Expected: `main.ts` ≤ 50 lines, each module ≤ 200 lines.

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat: complete modular refactor - store, canvas wires, idle chunking, forward/back nav, muted colors"
```
