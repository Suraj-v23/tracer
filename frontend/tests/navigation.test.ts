import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../api/api.js',   () => ({ getFilesystem: vi.fn(), getHomeDir: vi.fn() }));
vi.mock('../core/state.js', () => ({
    state: {
        currentPath: '', currentData: null,
        backStack: [], forwardStack: [],
        searchQuery: '', transform: { x: 0, y: 0, scale: 1 },
        selectedNode: null,
    }
}));
vi.mock('../core/store.js', () => ({
    get: vi.fn(() => null),
    set: vi.fn(),
    isStale: vi.fn(() => false),
    revalidate: vi.fn(),
    prefetch: vi.fn(),
}));

const mockBreadcrumb = { innerHTML: '', querySelectorAll: vi.fn(() => []) };
vi.stubGlobal('document', {
    getElementById: vi.fn((id: string) => {
        if (id === 'breadcrumb')    return mockBreadcrumb;
        if (id === 'loading')       return { classList: { remove: vi.fn(), add: vi.fn() } };
        if (id === 'loading-path')  return { textContent: '' };
        return null;
    }),
});

import { state } from '../core/state.js';
import { canGoBack, canGoForward, recordNavigate, back, forward } from '../components/navigation.js';

beforeEach(() => {
    state.currentPath  = '';
    state.backStack    = [];
    state.forwardStack = [];
});

describe('canGoBack / canGoForward', () => {
    it('returns false when stacks empty', () => {
        expect(canGoBack()).toBe(false);
        expect(canGoForward()).toBe(false);
    });
    it('canGoBack after recording navigation', () => {
        state.currentPath = '/home';
        recordNavigate('/home/docs');
        expect(canGoBack()).toBe(true);
    });
});

describe('recordNavigate', () => {
    it('pushes currentPath to backStack and clears forwardStack', () => {
        state.currentPath  = '/home';
        state.forwardStack = ['/prev'];
        recordNavigate('/home/docs');
        expect(state.backStack).toContain('/home');
        expect(state.forwardStack).toHaveLength(0);
    });
});

describe('back / forward', () => {
    it('back pops backStack, pushes current to forwardStack', () => {
        state.backStack    = ['/home'];
        state.currentPath  = '/home/docs';
        state.forwardStack = [];
        const path = back();
        expect(path).toBe('/home');
        expect(state.forwardStack).toContain('/home/docs');
        expect(state.backStack).toHaveLength(0);
    });
    it('forward pops forwardStack, pushes current to backStack', () => {
        state.backStack    = ['/home'];
        state.currentPath  = '/home/docs';
        state.forwardStack = ['/home/docs/sub'];
        const path = forward();
        expect(path).toBe('/home/docs/sub');
        expect(state.backStack).toContain('/home/docs');
    });
    it('back returns null when stack empty', () => {
        expect(back()).toBeNull();
    });
    it('forward returns null when stack empty', () => {
        expect(forward()).toBeNull();
    });
});
