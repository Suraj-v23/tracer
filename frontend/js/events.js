import { state } from './state.js';
import * as nav from './navigation.js';
import { applyFiltersAndRender, applySearch } from './search.js';
import { openSidebar, closeSidebar } from './sidebar.js';
import { centerWorkspace, updateTransform, redrawWires } from './canvas.js';
import { expandNodeInPlace, getNodesLayer, collapseExpansion, popAndCollapse } from './nodes.js';
import * as store from './store.js';
import * as api from './api.js';
import { showSendPanel } from './transfer.js';
import { UI_ICONS } from './icons.js';
import { addIndexedFolder, showImports, showSimilar } from './graphui.js';
export function toast(msg, type = '') {
    const el = document.createElement('div');
    el.className = `toast ${type}`;
    el.textContent = msg;
    document.getElementById('toast-container')?.appendChild(el);
    setTimeout(() => {
        el.style.animation = 'toastOut 0.25s forwards';
        setTimeout(() => el.remove(), 250);
    }, 2500);
}
const EDGE_SIZE = 80; // px from viewport edge that triggers auto-pan
const PAN_SPEED = 10; // px per frame
let _lastMouse = { x: 0, y: 0 };
let _edgePanId = null;
function startEdgePan() {
    if (_edgePanId !== null)
        return;
    _edgePanId = requestAnimationFrame(edgePanFrame);
}
function stopEdgePan() {
    if (_edgePanId !== null) {
        cancelAnimationFrame(_edgePanId);
        _edgePanId = null;
    }
}
function edgePanFrame() {
    if (!state.draggingNode) {
        stopEdgePan();
        return;
    }
    const { x: mx, y: my } = _lastMouse;
    let dx = 0, dy = 0;
    if (mx < EDGE_SIZE)
        dx = PAN_SPEED;
    if (mx > window.innerWidth - EDGE_SIZE)
        dx = -PAN_SPEED;
    if (my < EDGE_SIZE)
        dy = PAN_SPEED;
    if (my > window.innerHeight - EDGE_SIZE)
        dy = -PAN_SPEED;
    if (dx || dy) {
        state.transform.x += dx;
        state.transform.y += dy;
        // Keep node under cursor as canvas pans
        const s = state.transform.scale;
        state.draggingNode.style.left = `${parseFloat(state.draggingNode.style.left) - dx / s}px`;
        state.draggingNode.style.top = `${parseFloat(state.draggingNode.style.top) - dy / s}px`;
        updateTransform();
        redrawWires();
    }
    _edgePanId = requestAnimationFrame(edgePanFrame);
}
export function bindCanvasEvents() {
    const container = document.getElementById('canvas-container');
    container.addEventListener('mousedown', e => {
        if (e.button !== 0 && e.button !== 2)
            return;
        if (e.target.closest('.html-node'))
            return;
        state.isDragging = true;
        state.startDrag = { x: e.clientX - state.transform.x, y: e.clientY - state.transform.y };
        container.style.cursor = 'grabbing';
    });
    window.addEventListener('mousemove', e => {
        _lastMouse = { x: e.clientX, y: e.clientY };
        if (state.draggingNode) {
            state.nodeHasDragged = true;
            document.body.style.cursor = 'grabbing';
            const mouseX = (e.clientX - state.transform.x) / state.transform.scale;
            const mouseY = (e.clientY - state.transform.y) / state.transform.scale;
            state.draggingNode.style.left = `${mouseX - state.nodeDragOffset.x}px`;
            state.draggingNode.style.top = `${mouseY - state.nodeDragOffset.y}px`;
            redrawWires();
            startEdgePan();
            return;
        }
        if (!state.isDragging)
            return;
        state.transform.x = e.clientX - state.startDrag.x;
        state.transform.y = e.clientY - state.startDrag.y;
        updateTransform();
    });
    window.addEventListener('mouseup', () => {
        stopEdgePan();
        if (state.draggingNode) {
            state.draggingNode.style.transition = '';
            setTimeout(() => { state.draggingNode = null; }, 0);
        }
        state.isDragging = false;
        document.body.style.cursor = '';
        container.style.cursor = 'grab';
    });
    container.addEventListener('wheel', e => {
        if (e.target.closest('.html-node'))
            return;
        e.preventDefault();
        const delta = -e.deltaY * 0.001;
        const newScale = Math.min(Math.max(0.1, state.transform.scale * (1 + delta)), 3);
        const rect = container.getBoundingClientRect();
        const mouseX = e.clientX - rect.left;
        const mouseY = e.clientY - rect.top;
        state.transform.x = mouseX - (mouseX - state.transform.x) * (newScale / state.transform.scale);
        state.transform.y = mouseY - (mouseY - state.transform.y) * (newScale / state.transform.scale);
        state.transform.scale = newScale;
        updateTransform();
    }, { passive: false });
}
let _createType = 'file';
let _createBasePath = '';
function showCreateModal(type, basePath) {
    _createType = type;
    _createBasePath = basePath;
    const icon = document.getElementById('create-icon');
    const title = document.getElementById('create-title');
    const input = document.getElementById('create-input');
    icon.innerHTML = type === 'file' ? UI_ICONS.newFile : UI_ICONS.newFolder;
    title.textContent = type === 'file' ? 'New File' : 'New Folder';
    input.value = '';
    document.getElementById('create-modal').classList.remove('hidden');
    setTimeout(() => input.focus(), 50);
}
function exitMoveMode() {
    state.moveMode = false;
    state.moveSource = null;
    document.getElementById('move-indicator')?.classList.add('hidden');
}
async function executeMoveInto(destFolder) {
    const src = state.moveSource;
    const destPath = destFolder.path.replace(/\/$/, '') + '/' + src.name;
    try {
        await api.moveItem(src.path, destPath);
        store.invalidate(src.path);
        store.invalidate(state.currentPath);
        store.invalidate(destFolder.path);
        toast(`Moved ${src.name} → ${destFolder.name}`, 'success');
    }
    catch (err) {
        toast('Move failed: ' + err, 'error');
    }
    exitMoveMode();
    await nav.navigate(state.currentPath);
}
export function bindGlobalEvents() {
    window.addEventListener('click', e => {
        const target = e.target;
        const ctxMenu = document.getElementById('ctx-menu');
        if (!ctxMenu.contains(target))
            ctxMenu.classList.add('hidden');
        if (!document.getElementById('sidebar').contains(target)
            && !target.closest('#toolbar')
            && !target.closest('.html-node')) {
            closeSidebar();
        }
    });
    document.getElementById('ctx-open').addEventListener('click', async () => {
        const item = state.ctxTarget;
        document.getElementById('ctx-menu').classList.add('hidden');
        if (!item || item.type !== 'directory')
            return;
        const sourceEl = [...getNodesLayer().querySelectorAll('.html-node')]
            .find(el => el._fsNode?.path === item.path);
        if (!sourceEl)
            return;
        if (sourceEl._expanded) {
            toast('Already expanded', '');
            return;
        }
        try {
            const data = await api.getFilesystem(item.path, 2, true);
            if (data.children?.length) {
                expandNodeInPlace(data.children, sourceEl, handleNodeClick, bindNodeContextMenu);
            }
            else {
                toast('Folder is empty', '');
            }
        }
        catch (err) {
            toast(String(err), 'error');
        }
    });
    document.getElementById('ctx-open-new').addEventListener('click', () => {
        if (state.ctxTarget?.type === 'directory')
            api.openNewWindow(state.ctxTarget.path).catch(e => toast('Error: ' + e, 'error'));
        document.getElementById('ctx-menu').classList.add('hidden');
    });
    document.getElementById('ctx-collapse').addEventListener('click', () => {
        if (state.ctxTargetEl)
            collapseExpansion(state.ctxTargetEl);
        document.getElementById('ctx-menu').classList.add('hidden');
    });
    document.getElementById('ctx-info').addEventListener('click', () => {
        if (state.ctxTarget)
            openSidebar(state.ctxTarget);
        document.getElementById('ctx-menu').classList.add('hidden');
    });
    document.getElementById('ctx-delete').addEventListener('click', () => {
        if (state.ctxTarget)
            showDeleteModal(state.ctxTarget);
        document.getElementById('ctx-menu').classList.add('hidden');
    });
    document.getElementById('ctx-new-file').addEventListener('click', () => {
        const base = state.ctxTarget?.type === 'directory' ? state.ctxTarget.path : state.currentPath;
        showCreateModal('file', base);
        document.getElementById('ctx-menu').classList.add('hidden');
    });
    document.getElementById('ctx-new-folder').addEventListener('click', () => {
        const base = state.ctxTarget?.type === 'directory' ? state.ctxTarget.path : state.currentPath;
        showCreateModal('folder', base);
        document.getElementById('ctx-menu').classList.add('hidden');
    });
    document.getElementById('ctx-move').addEventListener('click', () => {
        if (state.ctxTarget) {
            state.moveMode = true;
            state.moveSource = state.ctxTarget;
            document.getElementById('move-indicator').classList.remove('hidden');
        }
        document.getElementById('ctx-menu').classList.add('hidden');
    });
    document.getElementById('ctx-send').addEventListener('click', async () => {
        const item = state.ctxTarget;
        document.getElementById('ctx-menu').classList.add('hidden');
        if (!item || item.type === 'directory') {
            toast('Select a file to send (folders not supported in v1)', '');
            return;
        }
        state.ctxSendPath = item.path;
        await showSendPanel(item.path, item.name);
    });
    document.getElementById('ctx-deep-index')?.addEventListener('click', async () => {
        document.getElementById('ctx-menu').classList.add('hidden');
        const item = state.ctxTarget;
        if (!item || item.type !== 'directory')
            return;
        await addIndexedFolder(item.path);
    });
    document.getElementById('ctx-show-imports')?.addEventListener('click', async () => {
        document.getElementById('ctx-menu').classList.add('hidden');
        if (state.ctxTarget)
            await showImports(state.ctxTarget.path, 'imports');
    });
    document.getElementById('ctx-show-importers')?.addEventListener('click', async () => {
        document.getElementById('ctx-menu').classList.add('hidden');
        if (state.ctxTarget)
            await showImports(state.ctxTarget.path, 'importers');
    });
    document.getElementById('ctx-find-similar')?.addEventListener('click', async () => {
        document.getElementById('ctx-menu').classList.add('hidden');
        if (state.ctxTarget)
            await showSimilar(state.ctxTarget.path);
    });
    document.getElementById('graph-indexed-close')?.addEventListener('click', () => {
        document.getElementById('graph-indexed-panel')?.classList.add('hidden');
    });
    document.getElementById('send-panel-close').addEventListener('click', () => {
        document.getElementById('send-panel').classList.add('hidden');
    });
    document.getElementById('btn-new-file')?.addEventListener('click', () => showCreateModal('file', state.currentPath));
    document.getElementById('btn-new-folder')?.addEventListener('click', () => showCreateModal('folder', state.currentPath));
    document.getElementById('btn-move-cancel')?.addEventListener('click', exitMoveMode);
    document.getElementById('btn-create-cancel').addEventListener('click', () => document.getElementById('create-modal').classList.add('hidden'));
    const createInput = document.getElementById('create-input');
    createInput.addEventListener('keydown', e => {
        if (e.key === 'Enter')
            document.getElementById('btn-create-confirm').click();
        if (e.key === 'Escape')
            document.getElementById('create-modal').classList.add('hidden');
    });
    document.getElementById('btn-create-confirm').addEventListener('click', async () => {
        const input = document.getElementById('create-input');
        const name = input.value.trim();
        if (!name)
            return;
        const fullPath = _createBasePath.replace(/\/$/, '') + '/' + name;
        try {
            if (_createType === 'file')
                await api.createFile(fullPath);
            else
                await api.createFolder(fullPath);
            store.invalidate(_createBasePath);
            store.invalidate(state.currentPath);
            toast(`Created ${name}`, 'success');
            document.getElementById('create-modal').classList.add('hidden');
            await nav.navigate(state.currentPath);
        }
        catch (err) {
            toast('Create failed: ' + err, 'error');
        }
    });
    document.getElementById('sb-delete').addEventListener('click', () => {
        const sidebar = document.getElementById('sidebar');
        const item = sidebar._currentItem;
        if (item)
            showDeleteModal(item);
    });
    document.getElementById('sidebar-close').addEventListener('click', closeSidebar);
    const searchInput = document.getElementById('graph-search-input');
    searchInput.addEventListener('input', e => applySearch(e.target.value.trim()));
    document.getElementById('search-clear').addEventListener('click', () => {
        searchInput.value = '';
        applySearch('');
        searchInput.focus();
    });
    const sortSelect = document.getElementById('sort-select');
    sortSelect.addEventListener('change', () => {
        state.sortMode = sortSelect.value;
        applyFiltersAndRender();
    });
    window.addEventListener('keydown', e => {
        const si = document.getElementById('graph-search-input');
        if (document.activeElement === si) {
            if (e.key === 'Escape') {
                si.blur();
                si.value = '';
                applySearch('');
            }
            return;
        }
        if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key)) {
            e.preventDefault();
            nav.navigateNodes(e.key);
            return;
        }
        if (e.key === 'Enter' && state.selectedNode) {
            state.selectedNode.click();
            return;
        }
        if (e.key === 'Backspace' && !e.shiftKey) {
            e.preventDefault();
            if (popAndCollapse())
                return;
            if (nav.canGoBack()) {
                const prev = nav.back();
                if (prev)
                    nav.navigate(prev);
            }
            return;
        }
        if ((e.key === ']' || (e.key === 'Backspace' && e.shiftKey)) && nav.canGoForward()) {
            e.preventDefault();
            const next = nav.forward();
            if (next)
                nav.navigate(next);
            return;
        }
        if (e.key === 'Escape') {
            closeSidebar();
            document.getElementById('ctx-menu').classList.add('hidden');
            document.getElementById('confirm-modal').classList.add('hidden');
            document.getElementById('send-panel').classList.add('hidden');
            document.getElementById('incoming-overlay').classList.add('hidden');
        }
        if (e.key === 'r' || e.key === 'R')
            centerWorkspace();
        if (e.key === 'F5' || ((e.metaKey || e.ctrlKey) && e.key === 'r')) {
            e.preventDefault();
            store.invalidate(state.currentPath);
            nav.navigate(state.currentPath);
        }
        if (e.key === '/') {
            e.preventDefault();
            si.focus();
        }
    });
    document.getElementById('nodes-layer').addEventListener('dblclick', e => {
        const nodeEl = e.target.closest('.html-node');
        if (!nodeEl)
            return;
        const item = nodeEl._fsNode;
        if (item?.type === 'directory') {
            api.openNewWindow(item.path).catch(err => toast('Error: ' + err, 'error'));
        }
    });
    document.getElementById('btn-back')?.addEventListener('click', () => {
        if (popAndCollapse())
            return;
        const prev = nav.back();
        if (prev)
            nav.navigate(prev);
    });
    document.getElementById('btn-forward')?.addEventListener('click', () => {
        const next = nav.forward();
        if (next)
            nav.navigate(next);
    });
}
export function showDeleteModal(item) {
    const confirmText = document.getElementById('confirm-text');
    const confirmModal = document.getElementById('confirm-modal');
    confirmText.innerHTML = `Delete <strong>${item.name}</strong>?`;
    confirmModal.classList.remove('hidden');
    document.getElementById('btn-delete').onclick = async () => {
        try {
            await api.deleteItem(item.path);
            store.invalidate(item.path);
            store.invalidate(state.currentPath);
            toast(`Deleted ${item.name}`, 'success');
            closeSidebar();
            await nav.navigate(state.currentPath);
        }
        catch (err) {
            toast('Error deleting: ' + err, 'error');
        }
        confirmModal.classList.add('hidden');
    };
    document.getElementById('btn-cancel').onclick = () => confirmModal.classList.add('hidden');
}
export function bindNodeContextMenu(item, e) {
    state.ctxTarget = item;
    state.ctxTargetEl = e.target.closest('.html-node');
    const isDir = item.type === 'directory';
    const isExpanded = !!state.ctxTargetEl?._expanded;
    document.getElementById('ctx-open').classList.toggle('hidden', !isDir);
    document.getElementById('ctx-open-new').classList.toggle('hidden', !isDir);
    document.getElementById('ctx-collapse').classList.toggle('hidden', !isExpanded);
    const deepIdx = document.getElementById('ctx-deep-index');
    if (deepIdx)
        deepIdx.style.display = item.type === 'directory' ? '' : 'none';
    const isCode = !!(item.extension && ['.ts', '.js', '.tsx', '.jsx', '.py', '.rs', '.go'].includes(item.extension));
    const importItems = ['ctx-show-imports', 'ctx-show-importers'];
    importItems.forEach(id => {
        const el = document.getElementById(id);
        if (el)
            el.style.display = isCode ? '' : 'none';
    });
    const similarEl = document.getElementById('ctx-find-similar');
    if (similarEl)
        similarEl.style.display = item.type === 'file' ? '' : 'none';
    const ctxMenu = document.getElementById('ctx-menu');
    ctxMenu.style.left = Math.min(e.clientX, window.innerWidth - 190) + 'px';
    ctxMenu.style.top = Math.min(e.clientY, window.innerHeight - 180) + 'px';
    ctxMenu.classList.remove('hidden');
}
export function handleNodeClick(item, el, isRoot) {
    if (state.moveMode && state.moveSource && item.type === 'directory' && item.path !== state.moveSource.path) {
        executeMoveInto(item);
        return;
    }
    openSidebar(item);
}
