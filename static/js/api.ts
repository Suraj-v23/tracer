import type { FsNode } from './types.js';

function _invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
    const tauri = (window as any).__TAURI_INTERNALS__;
    if (!tauri) {
        const msg = document.getElementById('loading');
        if (msg) {
            msg.innerHTML = '<div style="padding:40px;text-align:center;color:#fff">' +
                '<div style="font-size:2rem;margin-bottom:16px">⚠️</div>' +
                '<div>Run inside Tauri: <code>npm run dev</code></div></div>';
            msg.classList.remove('hidden');
        }
        throw new Error('Tauri runtime not available — open via npm run dev');
    }
    return tauri.invoke(cmd, args);
}

export async function getFilesystem(path: string, depth = 2, force = false): Promise<FsNode> {
    return _invoke('get_filesystem', { path, depth, force }) as Promise<FsNode>;
}

export async function deleteItem(path: string): Promise<void> {
    return _invoke('delete_item', { path }) as Promise<void>;
}

export async function getHomeDir(): Promise<string> {
    return _invoke('get_home_dir') as Promise<string>;
}
