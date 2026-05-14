import * as graphApi from './graph.js';
import type { GraphSearchResult, IndexStats } from './graph.js';
import { toast } from './events.js';

type SearchMode = 'filter' | 'search' | 'ask';
let _currentMode: SearchMode = 'filter';
let _onFilterMode: (() => void) | null = null;

// ─── Init ─────────────────────────────────────────────────────────────────────

export function initGraphUI(onFilterMode: () => void): void {
    _onFilterMode = onFilterMode;
    _bindModeButtons();
    _startStatusPolling();

    refreshIndexedFoldersList();

    const tauri = (window as any).__TAURI__;
    if (tauri?.event?.listen) {
        tauri.event.listen('graph-content-indexed', () => {
            refreshIndexedFoldersList();
            toast('Content indexing complete', 'success');
        });
        tauri.event.listen('graph-embeddings-ready', (count: number) => {
            toast(`Semantic index ready — ${count} files embedded`, 'success');
        });
    }
}

// ─── Search modes ────────────────────────────────────────────────────────────

function _bindModeButtons(): void {
    document.getElementById('search-mode-filter')?.addEventListener('click', () => setMode('filter'));
    document.getElementById('search-mode-search')?.addEventListener('click', () => setMode('search'));
    document.getElementById('search-mode-ask')?.addEventListener('click',   () => setMode('ask'));

    document.getElementById('graph-search-form')?.addEventListener('submit', async (e) => {
        e.preventDefault();
        const input = document.getElementById('graph-search-input') as HTMLInputElement;
        const query = input?.value.trim();
        if (!query) return;
        if (_currentMode === 'search') await _runSearch(query);
        else if (_currentMode === 'ask') await _runSearch(query);
    });
}

export function setMode(mode: SearchMode): void {
    _currentMode = mode;
    const bar = document.getElementById('graph-search-bar');
    bar?.setAttribute('data-mode', mode);

    ['filter', 'search', 'ask'].forEach(m => {
        document.getElementById(`search-mode-${m}`)?.classList.toggle('active', m === mode);
    });

    const input = document.getElementById('graph-search-input') as HTMLInputElement;
    if (input) {
        input.placeholder =
            mode === 'filter' ? 'Filter by name…' :
            mode === 'search' ? 'Search filesystem (size, type, date…)' :
                                'Ask a question about your files…';
    }

    if (mode === 'filter') {
        hideResultsPanel();
        _onFilterMode?.();
    }
}

// ─── Search execution ─────────────────────────────────────────────────────────

async function _runSearch(query: string): Promise<void> {
    _showResultsLoading();
    try {
        const results = await graphApi.graphSearch(query);
        _showResults(results, query);
    } catch (e) {
        hideResultsPanel();
        toast(`Search failed: ${e}`, 'error');
    }
}

// ─── Results panel ────────────────────────────────────────────────────────────

function _showResultsLoading(): void {
    const panel = document.getElementById('graph-results-panel')!;
    panel.classList.remove('hidden');
    panel.innerHTML = '<div class="graph-results-loading">Searching…</div>';
}

export function hideResultsPanel(): void {
    document.getElementById('graph-results-panel')?.classList.add('hidden');
}

function _escHtml(s: string): string {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function _showResults(results: GraphSearchResult[], query: string): void {
    const panel = document.getElementById('graph-results-panel')!;
    panel.classList.remove('hidden');

    if (!results.length) {
        panel.innerHTML = `<div class="graph-results-empty">No results for <em>${_escHtml(query)}</em></div>`;
        return;
    }

    const items = results.map(r => `
        <div class="graph-result-item" data-path="${_escHtml(r.path)}" title="${_escHtml(r.path)}">
            <span class="gr-icon">${r.kind === 'directory' ? '📁' : '📄'}</span>
            <span class="gr-name">${_escHtml(r.name)}</span>
            <span class="gr-size">${r.size_human}</span>
            ${r.snippet ? `<span class="gr-snippet">${_escHtml(r.snippet)}</span>` : ''}
        </div>
    `).join('');

    panel.innerHTML = `
        <div class="graph-results-header">
            <span>${results.length} result${results.length !== 1 ? 's' : ''}</span>
            <button id="graph-results-close" class="graph-results-close">✕</button>
        </div>
        <div class="graph-results-list">${items}</div>
    `;

    document.getElementById('graph-results-close')?.addEventListener('click', hideResultsPanel);
}

// ─── Index status bar ─────────────────────────────────────────────────────────

function _startStatusPolling(): void {
    _updateStatus();
    setInterval(_updateStatus, 3000);
}

async function _updateStatus(): Promise<void> {
    try {
        const stats: IndexStats = await graphApi.graphIndexStatus();
        _renderStatus(stats);
    } catch { /* graph not initialized yet */ }
}

function _renderStatus(stats: IndexStats): void {
    const bar = document.getElementById('graph-status-bar');
    if (!bar) return;
    if (stats.indexed === 0) { bar.textContent = ''; return; }
    const done = stats.indexed >= stats.total && stats.total > 0;
    bar.textContent = done
        ? `Graph: ${stats.indexed.toLocaleString()} files indexed`
        : `Indexing: ${stats.indexed.toLocaleString()} / ${stats.total.toLocaleString()} files…`;
    bar.classList.toggle('indexing', !done);
}

// ─── Root indexing trigger ────────────────────────────────────────────────────

export async function triggerIndex(path: string): Promise<void> {
    try {
        await graphApi.graphSetRoot(path);
    } catch (e) {
        console.error('[graph] index trigger failed:', e);
    }
}

// ─── Indexed Folders Panel ────────────────────────────────────────────────────

export async function showImports(path: string, mode: 'imports' | 'importers'): Promise<void> {
    const fn_ = mode === 'imports' ? graphApi.graphGetImports : graphApi.graphGetImporters;
    const label = mode === 'imports' ? 'imports' : 'imported by';
    const name = path.split('/').pop() || path;

    const panel = document.getElementById('graph-results-panel')!;
    panel.classList.remove('hidden');
    panel.innerHTML = '<div class="graph-results-loading">Loading…</div>';

    try {
        const results = await fn_(path);
        if (!results.length) {
            panel.innerHTML = `<div class="graph-results-empty">${_escHtml(name)} has no ${label}</div>`;
            return;
        }
        const items = results.map(r => `
            <div class="graph-result-item" data-path="${_escHtml(r.path)}" title="${_escHtml(r.path)}">
                <span class="gr-icon">${r.kind === 'directory' ? '📁' : '📄'}</span>
                <span class="gr-name">${_escHtml(r.name)}</span>
                <span class="gr-size">${r.size_human}</span>
            </div>
        `).join('');
        panel.innerHTML = `
            <div class="graph-results-header">
                <span>${_escHtml(name)} ${label} ${results.length} file${results.length !== 1 ? 's' : ''}</span>
                <button id="graph-results-close" class="graph-results-close">✕</button>
            </div>
            <div class="graph-results-list">${items}</div>
        `;
        document.getElementById('graph-results-close')?.addEventListener('click', hideResultsPanel);
    } catch (e) {
        panel.innerHTML = `<div class="graph-results-empty">Error: ${_escHtml(String(e))}</div>`;
    }
}

export async function showSimilar(path: string): Promise<void> {
    const name = path.split('/').pop() || path;
    const panel = document.getElementById('graph-results-panel')!;
    panel.classList.remove('hidden');
    panel.innerHTML = '<div class="graph-results-loading">Finding similar files…</div>';

    try {
        const results = await graphApi.graphFindSimilar(path, 10);
        if (!results.length) {
            panel.innerHTML = `<div class="graph-results-empty">No similar files found for ${_escHtml(name)}.<br><small>Deep-index + embed the folder first.</small></div>`;
            return;
        }
        const items = results.map(r => `
            <div class="graph-result-item" data-path="${_escHtml(r.path)}" title="${_escHtml(r.path)}">
                <span class="gr-icon">${r.kind === 'directory' ? '📁' : '📄'}</span>
                <span class="gr-name">${_escHtml(r.name)}</span>
                <span class="gr-size">${r.size_human}</span>
            </div>
        `).join('');
        panel.innerHTML = `
            <div class="graph-results-header">
                <span>Files similar to ${_escHtml(name)}</span>
                <button id="graph-results-close" class="graph-results-close">✕</button>
            </div>
            <div class="graph-results-list">${items}</div>
        `;
        document.getElementById('graph-results-close')?.addEventListener('click', hideResultsPanel);
    } catch (e) {
        const msg = String(e);
        if (msg.includes('No embedding provider')) {
            panel.innerHTML = `<div class="graph-results-empty">Set an embedding provider first.<br><small>${_escHtml(msg)}</small></div>`;
        } else {
            panel.innerHTML = `<div class="graph-results-empty">Error: ${_escHtml(msg)}</div>`;
        }
    }
}

export async function addIndexedFolder(path: string): Promise<void> {
    try {
        await graphApi.graphAddIndexedFolder(path);
        toast(`Indexing content in ${path.split('/').pop()}…`, '');
        await refreshIndexedFoldersList();
    } catch (e) {
        toast(`Failed to index folder: ${e}`, 'error');
    }
}

export async function refreshIndexedFoldersList(): Promise<void> {
    const list = document.getElementById('graph-indexed-folders-list');
    if (!list) return;

    let folders: string[] = [];
    try { folders = await graphApi.graphListIndexedFolders(); } catch { return; }

    list.innerHTML = folders.length === 0
        ? '<div class="graph-no-folders">No folders deep-indexed yet</div>'
        : folders.map(f => `
            <div class="graph-indexed-folder" data-path="${_escHtml(f)}">
                <span class="gif-name" title="${_escHtml(f)}">${_escHtml(f.split('/').pop() || f)}</span>
                <button class="gif-remove" data-path="${_escHtml(f)}" title="Remove">✕</button>
            </div>
          `).join('');

    list.querySelectorAll<HTMLElement>('.gif-remove').forEach(btn => {
        btn.addEventListener('click', async () => {
            const p = btn.dataset.path!;
            try {
                await graphApi.graphRemoveIndexedFolder(p);
                await refreshIndexedFoldersList();
            } catch (e) {
                toast(`Remove failed: ${e}`, 'error');
            }
        });
    });
}
