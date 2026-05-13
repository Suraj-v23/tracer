import type { FsNode } from './types.js';
import { UI_ICONS } from './icons.js';

function _invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
    const tauri = (window as any).__TAURI_INTERNALS__;
    if (!tauri) {
        const msg = document.getElementById('loading');
        if (msg) {
            msg.innerHTML = '<div style="padding:40px;text-align:center;color:#fff">' +
                `<div style="font-size:2rem;margin-bottom:16px">${UI_ICONS.warning}</div>` +
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

export async function createFile(path: string): Promise<void> {
    return _invoke('create_file', { path }) as Promise<void>;
}

export async function createFolder(path: string): Promise<void> {
    return _invoke('create_folder', { path }) as Promise<void>;
}

export async function moveItem(from: string, to: string): Promise<void> {
    return _invoke('move_item', { from, to }) as Promise<void>;
}

export async function openNewWindow(path: string): Promise<void> {
    return _invoke('open_in_new_window', { path }) as Promise<void>;
}

import type {
    PeerInfo,
    TransferSession,
} from './types.js';

export async function getPeers(): Promise<PeerInfo[]> {
    return _invoke('get_peers') as Promise<PeerInfo[]>;
}

export async function startTransfer(path: string, peerId: string): Promise<TransferSession> {
    return _invoke('start_transfer', { path, peerId }) as Promise<TransferSession>;
}

export async function acceptTransfer(sessionId: string, destPath: string): Promise<void> {
    return _invoke('accept_transfer', { sessionId, destPath }) as Promise<void>;
}

export async function rejectTransfer(sessionId: string): Promise<void> {
    return _invoke('reject_transfer', { sessionId }) as Promise<void>;
}

export async function cancelTransfer(sessionId: string): Promise<void> {
    return _invoke('cancel_transfer', { sessionId }) as Promise<void>;
}

export function listenEvent<T>(
    event: string,
    handler: (payload: T) => void
): Promise<() => void> {
    const tauri = (window as any).__TAURI__;
    if (!tauri?.event?.listen) return Promise.resolve(() => {});
    return tauri.event.listen(event, (e: { payload: T }) => handler(e.payload));
}
