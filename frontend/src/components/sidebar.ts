import type { FsNode } from '../core/types.js';
import { state } from '../core/state.js';
import { getFileCategory, TYPE_ICONS } from '../utils/utils.js';
import { navigate } from './navigation.js';

export function openSidebar(item: FsNode): void {
    const isDir = item.type === 'directory';
    const cat   = getFileCategory(item);

    _setHtml('sb-icon',   TYPE_ICONS[cat] ?? '📎');
    _set('sb-name',       item.name);
    _set('sb-size-badge', item.size_human);
    _set('sb-type',       isDir ? 'Folder' : (item.extension ?? 'file').replace('.', '').toUpperCase());
    _set('sb-size',       item.size_human);
    _set('sb-path',       item.path);
    _set('sb-modified',   item.modified_time);
    _set('sb-readonly',   item.readonly ? 'Yes' : 'No');

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
    setSidebarItem(item);
}

export function closeSidebar(): void {
    document.getElementById('sidebar')?.classList.add('hidden');
    if (state.selectedNode) {
        state.selectedNode.classList.remove('selected');
        state.selectedNode = null;
    }
}

export function setSidebarItem(item: FsNode): void {
    const sidebar = document.getElementById('sidebar');
    if (sidebar) (sidebar as any)._currentItem = item;
}

function _set(id: string, value: string): void {
    const el = document.getElementById(id);
    if (el) el.textContent = value;
}

function _setHtml(id: string, value: string): void {
    const el = document.getElementById(id);
    if (el) el.innerHTML = value;
}
