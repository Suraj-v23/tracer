import type { FsNode } from './types.js';

export const state = {
    // Navigation
    currentPath:  '',
    currentData:  null as FsNode | null,
    backStack:    [] as string[],
    forwardStack: [] as string[],

    // Filters
    activeFilter: 'all',
    searchQuery:  '',
    sortMode:     'size-desc',

    // Context menu target
    ctxTarget:   null as FsNode | null,
    ctxTargetEl: null as HTMLElement | null,

    // Move mode
    moveMode:   false,
    moveSource: null as FsNode | null,

    // Selected node element
    selectedNode: null as HTMLElement | null,

    // Canvas pan/zoom
    transform: { x: 100, y: 0, scale: 1 },

    // Canvas pan drag state
    isDragging:      false,
    startDrag:       { x: 0, y: 0 },

    // Node drag state
    draggingNode:    null as HTMLElement | null,
    nodeDragOffset:  { x: 0, y: 0 },
    nodeHasDragged:  false,
};
