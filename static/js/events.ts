import type { FsNode } from './types.js';
import { state } from './state.js';
import * as nav from './navigation.js';
import { applyFiltersAndRender, applySearch } from './search.js';
import { openSidebar, closeSidebar, setSidebarItem } from './sidebar.js';
import { centerWorkspace, updateTransform, redrawWires } from './canvas.js';
import * as store from './store.js';
import * as api from './api.js';

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

export function bindCanvasEvents(): void {
    const container = document.getElementById('canvas-container')!;

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
    window.addEventListener('click', e => {
        const target  = e.target as HTMLElement;
        const ctxMenu = document.getElementById('ctx-menu')!;
        if (!ctxMenu.contains(target)) ctxMenu.classList.add('hidden');
        if (!document.getElementById('sidebar')!.contains(target)
            && !target.closest('#toolbar')
            && !target.closest('.html-node')) {
            closeSidebar();
        }
    });

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

    const searchInput = document.getElementById('search-input') as HTMLInputElement;
    searchInput.addEventListener('input', e =>
        applySearch((e.target as HTMLInputElement).value.trim())
    );

    document.getElementById('search-clear')!.addEventListener('click', () => {
        searchInput.value = '';
        applySearch('');
        searchInput.focus();
    });

    const sortSelect = document.getElementById('sort-select') as HTMLSelectElement;
    sortSelect.addEventListener('change', () => {
        state.sortMode = sortSelect.value;
        applyFiltersAndRender();
    });

    window.addEventListener('keydown', e => {
        const si = document.getElementById('search-input') as HTMLInputElement;
        if (document.activeElement === si) {
            if (e.key === 'Escape') { si.blur(); si.value = ''; applySearch(''); }
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

        if (e.key === 'Backspace' && !e.shiftKey && nav.canGoBack()) {
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
        if (e.key === 'r' || e.key === 'R') centerWorkspace();
        if (e.key === 'F5' || ((e.metaKey || e.ctrlKey) && e.key === 'r')) {
            e.preventDefault();
            store.invalidate(state.currentPath);
            nav.navigate(state.currentPath);
        }
        if (e.key === '/') { e.preventDefault(); si.focus(); }
    });

    document.getElementById('btn-back')?.addEventListener('click', () => {
        const prev = nav.back();
        if (prev) nav.navigate(prev);
    });

    document.getElementById('btn-forward')?.addEventListener('click', () => {
        const next = nav.forward();
        if (next) nav.navigate(next);
    });
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

    document.getElementById('btn-cancel')!.onclick = () =>
        confirmModal.classList.add('hidden');
}

export function bindNodeContextMenu(item: FsNode, e: MouseEvent): void {
    state.ctxTarget = item;
    document.getElementById('ctx-open')!.classList.toggle('disabled', item.type !== 'directory');
    const ctxMenu = document.getElementById('ctx-menu')!;
    ctxMenu.style.left = Math.min(e.clientX, window.innerWidth  - 190) + 'px';
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
