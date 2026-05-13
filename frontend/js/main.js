import * as api from './api.js';
import * as nav from './navigation.js';
import * as search from './search.js';
import { centerWorkspace } from './canvas.js';
import { bindCanvasEvents, bindGlobalEvents, handleNodeClick, bindNodeContextMenu } from './events.js';
import { initTransfer } from './transfer.js';
async function init() {
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
