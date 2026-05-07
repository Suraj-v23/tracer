export const state = {
    // Navigation
    currentPath: '',
    currentData: null,
    backStack: [],
    forwardStack: [],
    // Filters
    activeFilter: 'all',
    searchQuery: '',
    sortMode: 'size-desc',
    // Context menu target
    ctxTarget: null,
    // Selected node element
    selectedNode: null,
    // Canvas pan/zoom
    transform: { x: 100, y: 0, scale: 1 },
    // Canvas pan drag state
    isDragging: false,
    startDrag: { x: 0, y: 0 },
    // Node drag state
    draggingNode: null,
    nodeDragOffset: { x: 0, y: 0 },
    nodeHasDragged: false,
};
