import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock api and state modules before importing store
vi.mock('./api.js', () => ({
    getFilesystem: vi.fn(),
}));
vi.mock('./state.js', () => ({
    state: { currentPath: '/test', searchQuery: '' },
}));

import * as store from './store.js';
import * as api from './api.js';

const mockNode = {
    name: 'test', path: '/test', type: 'directory' as const,
    size: 100, size_human: '100 B', modified_time: '2026-01-01',
    readonly: false, children: [],
};

beforeEach(() => {
    store.invalidate('/test');
    store.invalidate('/parent/child');
    vi.clearAllMocks();
});

describe('get / set', () => {
    it('returns null for unknown path', () => {
        expect(store.get('/nonexistent')).toBeNull();
    });
    it('returns stored node', () => {
        store.set('/test', mockNode);
        expect(store.get('/test')).toBe(mockNode);
    });
});

describe('isStale', () => {
    it('returns true for unknown path', () => {
        expect(store.isStale('/nonexistent')).toBe(true);
    });
    it('returns false immediately after set', () => {
        store.set('/test', mockNode);
        expect(store.isStale('/test')).toBe(false);
    });
});

describe('invalidate', () => {
    it('removes node and parent', () => {
        store.set('/parent/child', mockNode);
        store.set('/parent', mockNode);
        store.invalidate('/parent/child');
        expect(store.get('/parent/child')).toBeNull();
        expect(store.get('/parent')).toBeNull();
    });
});

describe('revalidate', () => {
    it('fetches and updates store', async () => {
        vi.mocked(api.getFilesystem).mockResolvedValue(mockNode);
        await store.revalidate('/test');
        expect(store.get('/test')).toBe(mockNode);
    });
    it('calls onUpdate when on current path', async () => {
        vi.mocked(api.getFilesystem).mockResolvedValue(mockNode);
        const onUpdate = vi.fn();
        await store.revalidate('/test', onUpdate);
        expect(onUpdate).toHaveBeenCalledWith(mockNode);
    });
    it('does not call onUpdate when on different path', async () => {
        vi.mocked(api.getFilesystem).mockResolvedValue(mockNode);
        const onUpdate = vi.fn();
        await store.revalidate('/other', onUpdate);
        expect(onUpdate).not.toHaveBeenCalled();
    });
    it('silently ignores fetch errors', async () => {
        vi.mocked(api.getFilesystem).mockRejectedValue(new Error('network'));
        await expect(store.revalidate('/test')).resolves.toBeUndefined();
    });
});
