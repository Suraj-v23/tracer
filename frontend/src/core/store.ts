import type { FsNode } from './types.js';
import * as api from '../api/api.js';
import { state } from './state.js';

const nodes      = new Map<string, FsNode>();
const timestamps = new Map<string, number>();
const STALE_MS   = 120_000; // 2 minutes

export function get(path: string): FsNode | null {
    return nodes.get(path) ?? null;
}

export function set(path: string, node: FsNode): void {
    nodes.set(path, node);
    timestamps.set(path, Date.now());
}

export function isStale(path: string): boolean {
    const ts = timestamps.get(path);
    return ts === undefined || Date.now() - ts > STALE_MS;
}

export function invalidate(path: string): void {
    nodes.delete(path);
    timestamps.delete(path);
    const parent = path.split('/').slice(0, -1).join('/');
    if (parent) {
        nodes.delete(parent);
        timestamps.delete(parent);
    }
}

export async function revalidate(
    path: string,
    onUpdate?: (node: FsNode) => void
): Promise<void> {
    try {
        const node = await api.getFilesystem(path, 2, true);
        set(path, node);
        if (path === state.currentPath) onUpdate?.(node);
    } catch (_) {}
}

export async function prefetch(paths: string[]): Promise<void> {
    const missing = paths.filter(p => !get(p)).slice(0, 6);
    for (const p of missing) {
        await new Promise(r => setTimeout(r, 150));
        try { set(p, await api.getFilesystem(p)); } catch (_) {}
    }
}
