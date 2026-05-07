import { state } from './state.js';
import { getFileCategory } from './utils.js';
import { renderScene } from './nodes.js';
import { updateWireDimming, redrawWires } from './canvas.js';
import { rerenderDimming } from './nodes.js';
let _onNodeClick = null;
let _onNodeContextMenu = null;
export function setCallbacks(onNodeClick, onNodeContextMenu) {
    _onNodeClick = onNodeClick;
    _onNodeContextMenu = onNodeContextMenu;
}
export function applyFiltersAndRender() {
    if (!state.currentData)
        return;
    const items = getFilteredSortedItems();
    updateMatchCount(items.length);
    renderScene(items, state.currentData, state.searchQuery, _onNodeClick, _onNodeContextMenu);
}
export function applySearch(query) {
    state.searchQuery = query;
    const searchClear = document.getElementById('search-clear');
    if (searchClear)
        searchClear.classList.toggle('hidden', !query);
    // Fast path: update dimming only, no full re-render
    updateWireDimming(query);
    rerenderDimming(query);
    redrawWires();
    updateMatchCount(getFilteredSortedItems().length);
}
function getFilteredSortedItems() {
    let items = [...(state.currentData?.children ?? [])];
    items.sort((a, b) => {
        if (state.sortMode === 'size-desc')
            return b.size - a.size;
        if (state.sortMode === 'size-asc')
            return a.size - b.size;
        if (state.sortMode === 'name-asc')
            return a.name.localeCompare(b.name);
        if (state.sortMode === 'name-desc')
            return b.name.localeCompare(a.name);
        if (state.sortMode === 'type')
            return getFileCategory(a).localeCompare(getFileCategory(b));
        return 0;
    });
    if (state.activeFilter !== 'all') {
        items = items.filter(i => state.activeFilter === 'directory'
            ? i.type === 'directory'
            : getFileCategory(i) === state.activeFilter);
    }
    // NOTE: search query dims nodes visually, does NOT filter them out here.
    return items;
}
function updateMatchCount(count) {
    const el = document.getElementById('match-count');
    if (!el)
        return;
    el.textContent = state.searchQuery
        ? `${count} match${count !== 1 ? 'es' : ''}`
        : '';
}
export function updateStats() {
    if (!state.currentData)
        return;
    const items = state.currentData.children ?? [];
    const folders = items.filter(i => i.type === 'directory').length;
    const files = items.length - folders;
    const ic = document.getElementById('item-count');
    const ts = document.getElementById('total-size');
    if (ic)
        ic.textContent = `${items.length} items (${folders} folders, ${files} files)`;
    if (ts)
        ts.textContent = state.currentData.size_human ?? '0 B';
}
