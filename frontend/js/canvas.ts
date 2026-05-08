import { state } from './state.js';

interface Wire {
    fromEl: HTMLElement;
    toEl:   HTMLElement;
    color:  string;
    dimmed: boolean;
}

const wireRegistry: Wire[] = [];
const MARGIN = 300; // padding around wire bounding box

function wiresCanvas(): HTMLCanvasElement {
    return document.getElementById('wires-layer') as HTMLCanvasElement;
}

export function getWorkspace(): HTMLElement {
    return document.getElementById('node-workspace') as HTMLElement;
}

export function updateTransform(): void {
    const { x, y, scale } = state.transform;
    getWorkspace().style.transform = `translate(${x}px, ${y}px) scale(${scale})`;
}

export function centerWorkspace(): void {
    state.transform.x     = 100;
    state.transform.y     = window.innerHeight / 2 - 200;
    state.transform.scale = 1;
    updateTransform();
}

export function resizeCanvas(_width: number, _height: number): void {
    // No-op: canvas is dynamically sized on each redrawWires() call
}

export function clearWires(): void {
    wireRegistry.length = 0;
    const c = wiresCanvas();
    c.width = 1; c.height = 1;
    c.style.left = '0px'; c.style.top = '0px';
}

export function registerWire(
    fromEl: HTMLElement,
    toEl:   HTMLElement,
    color:  string,
    dimmed: boolean
): void {
    wireRegistry.push({ fromEl, toEl, color, dimmed });
}

export function getChildElements(sourceEl: HTMLElement): HTMLElement[] {
    return wireRegistry.filter(w => w.fromEl === sourceEl).map(w => w.toEl);
}

export function removeWiresFrom(sourceEl: HTMLElement): void {
    for (let i = wireRegistry.length - 1; i >= 0; i--) {
        if (wireRegistry[i].fromEl === sourceEl) wireRegistry.splice(i, 1);
    }
}

export function updateWireDimming(query: string): void {
    for (const w of wireRegistry) {
        const name = (w.toEl as any)._itemName as string ?? '';
        w.dimmed = !!query && !name.toLowerCase().includes(query.toLowerCase());
    }
}

export function redrawWires(): void {
    if (!wireRegistry.length) return;

    const NODE_W = 200;

    // Compute bounding box of all wire endpoints in workspace coords
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const w of wireRegistry) {
        const x1 = parseFloat(w.fromEl.style.left) + NODE_W;
        const y1 = parseFloat(w.fromEl.style.top)  + 40;
        const x2 = parseFloat(w.toEl.style.left);
        const y2 = parseFloat(w.toEl.style.top)    + 40;
        if (x1 < minX) minX = x1; if (x1 > maxX) maxX = x1;
        if (y1 < minY) minY = y1; if (y1 > maxY) maxY = y1;
        if (x2 < minX) minX = x2; if (x2 > maxX) maxX = x2;
        if (y2 < minY) minY = y2; if (y2 > maxY) maxY = y2;
    }

    minX -= MARGIN; minY -= MARGIN;
    maxX += MARGIN; maxY += MARGIN;

    // Reposition and resize canvas to exactly cover the wire area
    const c = wiresCanvas();
    c.style.left = `${minX}px`;
    c.style.top  = `${minY}px`;
    c.width  = maxX - minX;
    c.height = maxY - minY;

    const ctx = c.getContext('2d')!;

    for (const w of wireRegistry) {
        // Translate coords to canvas-local space
        const x1 = parseFloat(w.fromEl.style.left) + NODE_W - minX;
        const y1 = parseFloat(w.fromEl.style.top)  + 40    - minY;
        const x2 = parseFloat(w.toEl.style.left)          - minX;
        const y2 = parseFloat(w.toEl.style.top)    + 40    - minY;

        const dist = Math.abs(x2 - x1) * 0.5;
        ctx.beginPath();
        ctx.moveTo(x1, y1);
        ctx.bezierCurveTo(x1 + dist, y1, x2 - dist, y2, x2, y2);
        ctx.strokeStyle = w.color;
        ctx.globalAlpha = w.dimmed ? 0.1 : 0.4;
        ctx.lineWidth   = 1.5;
        ctx.stroke();
    }
    ctx.globalAlpha = 1;
}
