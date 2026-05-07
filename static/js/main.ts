import * as api from './api.js';
import * as nav from './navigation.js';
import * as search from './search.js';
import { centerWorkspace } from './canvas.js';
import { bindCanvasEvents, bindGlobalEvents, handleNodeClick, bindNodeContextMenu } from './events.js';

async function init(): Promise<void> {
    nav.setOnNavigate(_node => {
        search.applyFiltersAndRender();
        search.updateStats();
    });

    search.setCallbacks(handleNodeClick, bindNodeContextMenu);

    bindCanvasEvents();
    bindGlobalEvents();
    centerWorkspace();

    const homeDir = await api.getHomeDir().catch(() => '/Users');
    await nav.navigate(homeDir);
    document.getElementById('loading')?.classList.add('hidden');
}

init();
