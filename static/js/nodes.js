import { state } from './state.js';
import { getColor, getFileCategory, TYPE_ICONS } from './utils.js';
import { registerWire, redrawWires, resizeCanvas, clearWires } from './canvas.js';
const COL_HEIGHT = 8;
const COL_WIDTH = 320;
const ROW_HEIGHT = 110;
export const ROOT_X = 100;
export const ROOT_Y = 200;
const NODE_WIDTH = 200;
const CHUNK_SIZE = 50;
let _rootEl = null;
export function getNodesLayer() {
    return document.getElementById('nodes-layer');
}
function getNodePosition(i, totalItems) {
    const col = Math.floor(i / COL_HEIGHT);
    const row = i % COL_HEIGHT;
    const colItems = Math.min(totalItems - col * COL_HEIGHT, COL_HEIGHT);
    return {
        x: ROOT_X + 400 + col * COL_WIDTH,
        y: ROOT_Y + (row - colItems / 2 + 0.5) * ROW_HEIGHT,
    };
}
export function renderScene(items, rootData, query, onNodeClick, onNodeContextMenu) {
    const nodesLayer = getNodesLayer();
    nodesLayer.innerHTML = '';
    clearWires();
    const numCols = Math.ceil(items.length / COL_HEIGHT) || 1;
    resizeCanvas(ROOT_X + 400 + numCols * COL_WIDTH + NODE_WIDTH + 50, ROOT_Y + COL_HEIGHT * ROW_HEIGHT + 100);
    _rootEl = createNodeEl(rootData, ROOT_X, ROOT_Y, null, true, onNodeClick, onNodeContextMenu);
    nodesLayer.appendChild(_rootEl);
    if (!items.length)
        return;
    appendChunk(items, 0, Math.min(CHUNK_SIZE, items.length), query, nodesLayer, onNodeClick, onNodeContextMenu);
    redrawWires();
    if (items.length > CHUNK_SIZE) {
        scheduleChunk(items, CHUNK_SIZE, query, nodesLayer, onNodeClick, onNodeContextMenu);
    }
}
function appendChunk(items, start, end, query, nodesLayer, onNodeClick, onNodeContextMenu) {
    const frag = document.createDocumentFragment();
    for (let i = start; i < end; i++) {
        const item = items[i];
        const { x, y } = getNodePosition(i, items.length);
        const dimmed = !!query && !item.name.toLowerCase().includes(query.toLowerCase());
        const el = createNodeEl(item, x, y, query, false, onNodeClick, onNodeContextMenu);
        el.style.animationDelay = `${(i - start) * 15}ms`;
        registerWire(_rootEl, el, getColor(item), dimmed);
        frag.appendChild(el);
    }
    nodesLayer.appendChild(frag);
}
function scheduleChunk(items, startIdx, query, nodesLayer, onNodeClick, onNodeContextMenu) {
    requestIdleCallback(() => {
        const end = Math.min(startIdx + CHUNK_SIZE, items.length);
        appendChunk(items, startIdx, end, query, nodesLayer, onNodeClick, onNodeContextMenu);
        redrawWires();
        if (end < items.length) {
            scheduleChunk(items, end, query, nodesLayer, onNodeClick, onNodeContextMenu);
        }
    });
}
export function createNodeEl(item, x, y, query, isRoot, onNodeClick, onNodeContextMenu) {
    const el = document.createElement('div');
    el.className = isRoot ? 'html-node root-node' : 'html-node node-enter';
    const color = isRoot ? 'var(--accent)' : getColor(item);
    const cat = getFileCategory(item);
    const icon = TYPE_ICONS[cat] ?? '📎';
    const dimmed = !isRoot && !!query && !item.name.toLowerCase().includes(query.toLowerCase());
    if (dimmed)
        el.classList.add('dimmed');
    el._itemName = item.name;
    el.style.borderLeftColor = color;
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
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
        if (e.button !== 0)
            return;
        e.stopPropagation();
        const mouseX = (e.clientX - state.transform.x) / state.transform.scale;
        const mouseY = (e.clientY - state.transform.y) / state.transform.scale;
        state.nodeDragOffset.x = mouseX - parseFloat(el.style.left);
        state.nodeDragOffset.y = mouseY - parseFloat(el.style.top);
        state.draggingNode = el;
        state.nodeHasDragged = false;
        el.style.transition = 'none';
    });
    el.addEventListener('click', (e) => {
        e.stopPropagation();
        if (state.nodeHasDragged) {
            state.nodeHasDragged = false;
            return;
        }
        if (state.selectedNode)
            state.selectedNode.classList.remove('selected');
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
export function rerenderDimming(query) {
    const nodesLayer = getNodesLayer();
    nodesLayer.querySelectorAll('.html-node:not(.root-node)').forEach(el => {
        const name = el._itemName ?? '';
        const dimmed = !!query && !name.toLowerCase().includes(query.toLowerCase());
        el.classList.toggle('dimmed', dimmed);
    });
}
