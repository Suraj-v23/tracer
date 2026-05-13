import * as api from './api.js';
import * as nav from './navigation.js';
import * as search from './search.js';
import { centerWorkspace } from './canvas.js';
import { bindCanvasEvents, bindGlobalEvents, handleNodeClick, bindNodeContextMenu, toast } from './events.js';
import { initTransfer } from './transfer.js';
import { UI_ICONS } from './icons.js';
function _applyIcons() {
    const set = (id, html) => {
        const el = document.getElementById(id);
        if (el)
            el.innerHTML = html;
    };
    set('btn-back', UI_ICONS.back);
    set('btn-forward', UI_ICONS.forward);
    set('search-clear', UI_ICONS.close);
    set('sidebar-close', UI_ICONS.close);
    set('send-panel-close', UI_ICONS.close);
    set('btn-new-file', UI_ICONS.newFile);
    set('btn-new-folder', UI_ICONS.newFolder);
    set('sb-icon', UI_ICONS.newFile);
    set('create-icon', UI_ICONS.newFile);
    const searchIcon = document.querySelector('.search-icon');
    if (searchIcon)
        searchIcon.innerHTML = UI_ICONS.search;
    const warnIcon = document.querySelector('#confirm-modal .modal-icon');
    if (warnIcon)
        warnIcon.innerHTML = UI_ICONS.warning;
    const ctxFile = document.getElementById('ctx-new-file');
    if (ctxFile)
        ctxFile.innerHTML = UI_ICONS.newFile + ' New File';
    const ctxFolder = document.getElementById('ctx-new-folder');
    if (ctxFolder)
        ctxFolder.innerHTML = UI_ICONS.newFolder + ' New Folder';
}
async function init() {
    _applyIcons();
    nav.setOnError(msg => toast(msg, 'error'));
    nav.setOnNavigate(_node => {
        search.applyFiltersAndRender();
        search.updateStats();
    });
    search.setCallbacks(handleNodeClick, bindNodeContextMenu);
    bindCanvasEvents();
    bindGlobalEvents();
    centerWorkspace();
    // Transfer init is non-critical — don't block app startup
    initTransfer().catch(e => console.error('[transfer] init failed:', e));
    const params = new URLSearchParams(window.location.search);
    const pathParam = params.get('path');
    const startPath = pathParam || await api.getHomeDir().catch(() => '/Users');
    await nav.navigate(startPath);
    document.getElementById('loading')?.classList.add('hidden');
}
init();
