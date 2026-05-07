import { state } from './state.js';
import { getFileCategory, TYPE_ICONS } from './utils.js';
import { navigate } from './navigation.js';
export function openSidebar(item) {
    const isDir = item.type === 'directory';
    const cat = getFileCategory(item);
    _set('sb-icon', TYPE_ICONS[cat] ?? '📎');
    _set('sb-name', item.name);
    _set('sb-size-badge', item.size_human);
    _set('sb-type', isDir ? 'Folder' : (item.extension ?? 'file').replace('.', '').toUpperCase());
    _set('sb-size', item.size_human);
    _set('sb-path', item.path);
    _set('sb-modified', item.modified_time);
    _set('sb-readonly', item.readonly ? 'Yes' : 'No');
    const sbEnter = document.getElementById('sb-enter');
    if (sbEnter) {
        if (isDir) {
            sbEnter.classList.remove('hidden');
            sbEnter.onclick = () => navigate(item.path);
        }
        else {
            sbEnter.classList.add('hidden');
        }
    }
    document.getElementById('sidebar')?.classList.remove('hidden');
    setSidebarItem(item);
}
export function closeSidebar() {
    document.getElementById('sidebar')?.classList.add('hidden');
    if (state.selectedNode) {
        state.selectedNode.classList.remove('selected');
        state.selectedNode = null;
    }
}
export function setSidebarItem(item) {
    const sidebar = document.getElementById('sidebar');
    if (sidebar)
        sidebar._currentItem = item;
}
function _set(id, value) {
    const el = document.getElementById(id);
    if (el)
        el.textContent = value;
}
