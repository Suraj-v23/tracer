import { state } from './state.js';
import * as store from './store.js';
import * as api from './api.js';
let _onNavigate = null;
export function setOnNavigate(fn) {
    _onNavigate = fn;
}
export function canGoBack() { return state.backStack.length > 0; }
export function canGoForward() { return state.forwardStack.length > 0; }
export function recordNavigate(newPath) {
    if (state.currentPath)
        state.backStack.push(state.currentPath);
    state.forwardStack = [];
}
export function back() {
    if (!canGoBack())
        return null;
    const prev = state.backStack.pop();
    state.forwardStack.push(state.currentPath);
    return prev;
}
export function forward() {
    if (!canGoForward())
        return null;
    const next = state.forwardStack.pop();
    state.backStack.push(state.currentPath);
    return next;
}
export async function navigate(path) {
    recordNavigate(path);
    const cached = store.get(path);
    if (cached) {
        _setCurrentAndRender(path, cached);
        if (store.isStale(path))
            store.revalidate(path, n => _setCurrentAndRender(path, n));
        return;
    }
    showLoading(path);
    try {
        const node = await api.getFilesystem(path);
        store.set(path, node);
        _setCurrentAndRender(path, node);
    }
    catch (e) {
        console.error(e);
    }
    finally {
        hideLoading();
    }
}
function _setCurrentAndRender(path, node) {
    state.currentPath = path;
    state.currentData = node;
    updateBreadcrumb(path);
    _onNavigate?.(node);
    const childDirs = (node.children ?? [])
        .filter(c => c.type === 'directory')
        .map(c => c.path);
    store.prefetch(childDirs);
}
function showLoading(path) {
    const el = document.getElementById('loading');
    const pathEl = document.getElementById('loading-path');
    if (el)
        el.classList.remove('hidden');
    if (pathEl)
        pathEl.textContent = path;
}
function hideLoading() {
    document.getElementById('loading')?.classList.add('hidden');
}
export function updateBreadcrumb(path) {
    const breadcrumb = document.getElementById('breadcrumb');
    if (!breadcrumb)
        return;
    const parts = path.split('/').filter(Boolean);
    let html = `<span class="crumb" data-path="/">/</span>`;
    let cur = '';
    for (const p of parts) {
        cur += '/' + p;
        html += `<span class="crumb-sep">/</span><span class="crumb" data-path="${cur}">${p}</span>`;
    }
    breadcrumb.innerHTML = html;
    breadcrumb.querySelectorAll('.crumb').forEach(el => {
        el.addEventListener('click', e => {
            e.stopPropagation();
            navigate(el.dataset.path);
        });
    });
}
export function navigateNodes(key) {
    const nodesLayer = document.getElementById('nodes-layer');
    if (!nodesLayer)
        return;
    const allNodes = Array.from(nodesLayer.querySelectorAll('.html-node:not(.root-node):not(.dimmed)'));
    if (!allNodes.length)
        return;
    const currentIdx = state.selectedNode
        ? allNodes.indexOf(state.selectedNode)
        : -1;
    let nextIdx = currentIdx;
    const COL_HEIGHT = 8;
    if (key === 'ArrowRight')
        nextIdx = currentIdx + COL_HEIGHT;
    if (key === 'ArrowLeft')
        nextIdx = currentIdx - COL_HEIGHT;
    if (key === 'ArrowDown')
        nextIdx = currentIdx + 1;
    if (key === 'ArrowUp')
        nextIdx = currentIdx - 1;
    nextIdx = Math.max(0, Math.min(nextIdx, allNodes.length - 1));
    if (nextIdx === currentIdx && currentIdx !== -1)
        return;
    if (state.selectedNode)
        state.selectedNode.classList.remove('selected');
    const next = allNodes[nextIdx === -1 ? 0 : nextIdx];
    next.classList.add('selected');
    state.selectedNode = next;
    next.scrollIntoView?.({ block: 'nearest' });
}
