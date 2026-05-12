import type {
    PeerInfo,
    TransferSession,
    IncomingTransferPayload,
    TransferProgressPayload,
    TransferCompletePayload,
    TransferErrorPayload,
} from './types.js';
import { state } from './state.js';
import * as api from './api.js';
import * as nav from './navigation.js';
import { toast } from './events.js';

let _currentSession: TransferSession | null = null;
let _incomingSession: IncomingTransferPayload | null = null;

// ── Send Panel ────────────────────────────────────────────────────────────────

export async function showSendPanel(filePath: string, filename: string): Promise<void> {
    const panel = document.getElementById('send-panel')!;
    document.getElementById('send-filename')!.textContent = filename;
    document.getElementById('send-code')!.textContent = '';
    document.getElementById('send-status')!.textContent = '';
    panel.classList.remove('hidden');
    await refreshPeerList();
}

export function hideSendPanel(): void {
    document.getElementById('send-panel')!.classList.add('hidden');
    _currentSession = null;
}

async function refreshPeerList(): Promise<void> {
    const container = document.getElementById('send-peers')!;
    const noPeers = document.getElementById('send-no-peers')!;
    container.innerHTML = '';

    let peers: PeerInfo[] = [];
    try {
        peers = await api.getPeers();
    } catch { /* network not available */ }

    noPeers.classList.toggle('hidden', peers.length > 0);

    for (const peer of peers) {
        const btn = document.createElement('button');
        btn.className = 'sb-btn';
        btn.style.cssText = 'width:100%;text-align:left;padding:8px 10px;font-size:0.82rem;';
        btn.textContent = `● ${peer.name}`;
        btn.dataset.peerId = peer.id;
        btn.addEventListener('click', () => sendToPeer(peer));
        container.appendChild(btn);
    }
}

async function sendToPeer(peer: PeerInfo): Promise<void> {
    const filePath = (state as any).ctxSendPath as string;
    if (!filePath) return;

    const status = document.getElementById('send-status')!;
    status.textContent = 'Sending offer…';

    try {
        const session = await api.startTransfer(filePath, peer.id);
        _currentSession = session;
        document.getElementById('send-code')!.textContent = session.code;
        status.textContent = `Waiting for ${peer.name} to accept…`;
    } catch (e) {
        status.textContent = `Error: ${e}`;
        toast(`Transfer failed: ${e}`, 'error');
    }
}

// ── Incoming Transfer Overlay ─────────────────────────────────────────────────

function showIncomingOverlay(payload: IncomingTransferPayload): void {
    _incomingSession = payload;
    const overlay = document.getElementById('incoming-overlay')!;
    document.getElementById('incoming-title')!.textContent =
        `${payload.sender_name} wants to send ${payload.filename}`;
    document.getElementById('incoming-meta')!.textContent =
        formatBytes(payload.size);
    document.getElementById('incoming-dest')!.textContent = state.currentPath;
    document.getElementById('incoming-actions')!.classList.remove('hidden');
    document.getElementById('incoming-progress-wrap')!.classList.add('hidden');
    overlay.classList.remove('hidden');
}

function hideIncomingOverlay(): void {
    document.getElementById('incoming-overlay')!.classList.add('hidden');
    _incomingSession = null;
}

function formatBytes(bytes: number): string {
    if (bytes < 1000) return `${bytes} B`;
    if (bytes < 1_000_000) return `${(bytes / 1000).toFixed(1)} KB`;
    if (bytes < 1_000_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
    return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
}

// ── Event Wiring ──────────────────────────────────────────────────────────────

export async function initTransfer(): Promise<void> {
    // Peer list updates
    await api.listenEvent<PeerInfo>('peer-discovered', () => {
        if (!document.getElementById('send-panel')!.classList.contains('hidden')) {
            refreshPeerList();
        }
    });

    await api.listenEvent<string>('peer-lost', () => {
        if (!document.getElementById('send-panel')!.classList.contains('hidden')) {
            refreshPeerList();
        }
    });

    // Incoming transfer request
    await api.listenEvent<IncomingTransferPayload>('incoming-transfer', (payload) => {
        showIncomingOverlay(payload);
    });

    // Transfer progress
    await api.listenEvent<TransferProgressPayload>('transfer-progress', (payload) => {
        const bar = document.getElementById('incoming-progress-bar');
        if (bar && payload.total > 0) {
            bar.style.width = `${Math.round((payload.bytes_done / payload.total) * 100)}%`;
        }
    });

    // Transfer complete
    await api.listenEvent<TransferCompletePayload>('transfer-complete', (payload) => {
        hideIncomingOverlay();
        hideSendPanel();
        toast(`Received: ${payload.saved_path.split('/').pop()}`, 'success');
        nav.navigate(state.currentPath);
    });

    // Transfer error
    await api.listenEvent<TransferErrorPayload>('transfer-error', (payload) => {
        hideIncomingOverlay();
        toast(`Transfer error: ${payload.error}`, 'error');
    });

    // Accept button
    document.getElementById('btn-accept-transfer')!.addEventListener('click', async () => {
        if (!_incomingSession) return;
        const sid = _incomingSession.session_id;
        document.getElementById('incoming-actions')!.classList.add('hidden');
        document.getElementById('incoming-progress-wrap')!.classList.remove('hidden');
        try {
            await api.acceptTransfer(sid, state.currentPath);
        } catch (e) {
            hideIncomingOverlay();
            toast(`Accept failed: ${e}`, 'error');
        }
    });

    // Decline button
    document.getElementById('btn-reject-transfer')!.addEventListener('click', async () => {
        if (!_incomingSession) return;
        await api.rejectTransfer(_incomingSession.session_id).catch(() => {});
        hideIncomingOverlay();
    });

    // Close send panel
    document.getElementById('send-panel-close')!.addEventListener('click', () => {
        if (_currentSession) {
            api.cancelTransfer(_currentSession.id).catch(() => {});
        }
        hideSendPanel();
    });
}
