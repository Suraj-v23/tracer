import type { FsNode } from '../core/types.js';
import { state } from '../core/state.js';
import { sizeToColor, getFileCategory, TYPE_ICONS } from '../utils/utils.js';
import { registerWire, redrawWires, resizeCanvas, clearWires, getChildElements, removeWiresFrom } from './canvas.js';

const COL_HEIGHT  = 8;
const COL_WIDTH   = 320;
const ROW_HEIGHT  = 110;
export const ROOT_X = 100;
export const ROOT_Y = 200;
const NODE_WIDTH  = 200;
const CHUNK_SIZE  = 50;

let _rootEl: HTMLElement | null = null;
const _expansionStack: HTMLElement[] = [];

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
    _expansionStack.length = 0;

    const numCols = Math.ceil(items.length / COL_HEIGHT) || 1;
    resizeCanvas(
        ROOT_X + 400 + numCols * COL_WIDTH + NODE_WIDTH + 50,
        ROOT_Y + COL_HEIGHT * ROW_HEIGHT + 100
    );

    _rootEl = createNodeEl(rootData, ROOT_X, ROOT_Y, null, true, 0, onNodeClick, onNodeContextMenu);
    nodesLayer.appendChild(_rootEl);

    if (!items.length) return;

    const maxSize = Math.max(...items.map(i => i.size), 1);

    appendChunk(items, 0, Math.min(CHUNK_SIZE, items.length), query, maxSize, nodesLayer, onNodeClick, onNodeContextMenu);
    redrawWires();

    if (items.length > CHUNK_SIZE) {
        scheduleChunk(items, CHUNK_SIZE, query, maxSize, nodesLayer, onNodeClick, onNodeContextMenu);
    }
}

function appendChunk(
    items: FsNode[],
    start: number,
    end: number,
    query: string,
    maxSize: number,
    nodesLayer: HTMLElement,
    onNodeClick: (item: FsNode, el: HTMLElement, isRoot: boolean) => void,
    onNodeContextMenu: (item: FsNode, e: MouseEvent) => void
): void {
    const frag = document.createDocumentFragment();
    for (let i = start; i < end; i++) {
        const item      = items[i];
        const { x, y }  = getNodePosition(i, items.length);
        const dimmed    = !!query && !item.name.toLowerCase().includes(query.toLowerCase());
        const el        = createNodeEl(item, x, y, query, false, maxSize, onNodeClick, onNodeContextMenu);
        el.style.animationDelay = `${(i - start) * 15}ms`;
        registerWire(_rootEl!, el, sizeToColor(item.size, maxSize), dimmed);
        frag.appendChild(el);
    }
    nodesLayer.appendChild(frag);
}

function scheduleChunk(
    items: FsNode[],
    startIdx: number,
    query: string,
    maxSize: number,
    nodesLayer: HTMLElement,
    onNodeClick: (item: FsNode, el: HTMLElement, isRoot: boolean) => void,
    onNodeContextMenu: (item: FsNode, e: MouseEvent) => void
): void {
    requestIdleCallback(() => {
        const end = Math.min(startIdx + CHUNK_SIZE, items.length);
        appendChunk(items, startIdx, end, query, maxSize, nodesLayer, onNodeClick, onNodeContextMenu);
        redrawWires();
        if (end < items.length) {
            scheduleChunk(items, end, query, maxSize, nodesLayer, onNodeClick, onNodeContextMenu);
        }
    });
}

export function createNodeEl(
    item: FsNode,
    x: number,
    y: number,
    query: string | null,
    isRoot: boolean,
    maxSize: number,
    onNodeClick: (item: FsNode, el: HTMLElement, isRoot: boolean) => void,
    onNodeContextMenu: (item: FsNode, e: MouseEvent) => void
): HTMLElement {
    const el    = document.createElement('div');
    el.className = isRoot ? 'html-node root-node' : 'html-node node-enter';
    const color  = isRoot ? 'var(--accent)' : sizeToColor(item.size, maxSize);
    const cat    = getFileCategory(item);
    const icon   = TYPE_ICONS[cat] ?? '📎';
    const dimmed = !isRoot && !!query && !item.name.toLowerCase().includes(query.toLowerCase());

    if (dimmed) el.classList.add('dimmed');

    (el as any)._itemName = item.name;
    (el as any)._fsNode  = item;

    el.style.borderLeftColor = color;
    el.style.left = `${x}px`;
    el.style.top  = `${y}px`;

    const typeLabel = item.type === 'directory'
        ? `Folder${item.children_count !== undefined ? ` · ${item.children_count} items` : ''}`
        : (item.extension ?? 'File');

    el.innerHTML = `
        <div class="node-header" style="background:${color}20; border-bottom-color:${color}40">
            <span class="node-icon">${icon}</span>
            <span class="node-title" title="${item.name}">${item.name}</span>
        </div>
        <div class="node-body">
            <div class="node-detail-row">
                <span class="node-detail">${typeLabel}</span>
                <span class="node-detail size">${item.size_human}</span>
            </div>
            <div class="node-detail-row node-meta">
                <span title="Modified">✎ ${item.modified_time}</span>
                ${item.readonly ? '<span style="color:var(--danger)">Read-only</span>' : ''}
            </div>
            <div class="node-detail-row node-meta">
                <span title="Created">⊕ ${item.created_time}</span>
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
        state.draggingNode   = el;
        state.nodeHasDragged = false;
        el.style.transition  = 'none';
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

function _freeStartX(sx: number, sy: number, childCount: number, nodesLayer: HTMLElement): number {
    const numNewCols = Math.ceil(childCount / COL_HEIGHT);
    const neededW    = numNewCols * COL_WIDTH;
    const halfH      = Math.min(childCount, COL_HEIGHT) / 2 * ROW_HEIGHT;
    const newMinY    = sy - halfH - 20;
    const newMaxY    = sy + halfH + 20;

    const existing = [...nodesLayer.querySelectorAll<HTMLElement>('.html-node')].map(el => ({
        left:  parseFloat(el.style.left)  || 0,
        right: (parseFloat(el.style.left) || 0) + NODE_WIDTH + 20,
        top:   parseFloat(el.style.top)   || 0,
        bot:   (parseFloat(el.style.top)  || 0) + 120,
    }));

    let candidateX = sx + 400;
    for (let attempt = 0; attempt < 30; attempt++) {
        const overlaps = existing.some(n =>
            n.right > candidateX &&
            n.left  < candidateX + neededW &&
            n.bot   > newMinY &&
            n.top   < newMaxY
        );
        if (!overlaps) break;
        candidateX += COL_WIDTH;
    }
    return candidateX;
}

export function expandNodeInPlace(
    children: FsNode[],
    sourceEl: HTMLElement,
    onNodeClick: (item: FsNode, el: HTMLElement, isRoot: boolean) => void,
    onNodeContextMenu: (item: FsNode, e: MouseEvent) => void
): void {
    if (!children.length) return;
    if ((sourceEl as any)._expanded) return;
    (sourceEl as any)._expanded = true;

    const sx       = parseFloat(sourceEl.style.left);
    const sy       = parseFloat(sourceEl.style.top);
    const maxSize  = Math.max(...children.map(c => c.size), 1);
    const nodesLayer = getNodesLayer();
    const startX   = _freeStartX(sx, sy, children.length, nodesLayer);
    const frag     = document.createDocumentFragment();

    children.forEach((item, i) => {
        const col      = Math.floor(i / COL_HEIGHT);
        const row      = i % COL_HEIGHT;
        const colItems = Math.min(children.length - col * COL_HEIGHT, COL_HEIGHT);
        const x = startX + col * COL_WIDTH;
        const y = sy + (row - colItems / 2 + 0.5) * ROW_HEIGHT;
        const el = createNodeEl(item, x, y, null, false, maxSize, onNodeClick, onNodeContextMenu);
        el.style.animationDelay = `${i * 15}ms`;
        registerWire(sourceEl, el, sizeToColor(item.size, maxSize), false);
        frag.appendChild(el);
    });

    _expansionStack.push(sourceEl);
    nodesLayer.appendChild(frag);
    redrawWires();
}

export function collapseExpansion(sourceEl: HTMLElement): void {
    if (!(sourceEl as any)._expanded) return;
    getChildElements(sourceEl).forEach(child => {
        collapseExpansion(child);
        child.remove();
    });
    removeWiresFrom(sourceEl);
    (sourceEl as any)._expanded = false;
    redrawWires();
}

export function popAndCollapse(): boolean {
    while (_expansionStack.length > 0) {
        const el = _expansionStack[_expansionStack.length - 1];
        _expansionStack.pop();
        if (el.isConnected && (el as any)._expanded) {
            collapseExpansion(el);
            return true;
        }
    }
    return false;
}

export function rerenderDimming(query: string): void {
    const nodesLayer = getNodesLayer();
    nodesLayer.querySelectorAll<HTMLElement>('.html-node:not(.root-node)').forEach(el => {
        const name   = (el as any)._itemName as string ?? '';
        const dimmed = !!query && !name.toLowerCase().includes(query.toLowerCase());
        el.classList.toggle('dimmed', dimmed);
    });
}
