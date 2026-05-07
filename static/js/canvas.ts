import { state } from './state.js';

interface Wire {
    fromEl: HTMLElement;
    toEl:   HTMLElement;
    color:  string;
    dimmed: boolean;
}

const wireRegistry: Wire[] = [];

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

export function resizeCanvas(width: number, height: number): void {
    const c  = wiresCanvas();
    c.width  = Math.max(width,  window.innerWidth);
    c.height = Math.max(height, window.innerHeight);
}

export function clearWires(): void {
    wireRegistry.length = 0;
    const c   = wiresCanvas();
    const ctx = c.getContext('2d')!;
    ctx.clearRect(0, 0, c.width, c.height);
}

export function registerWire(
    fromEl: HTMLElement,
    toEl:   HTMLElement,
    color:  string,
    dimmed: boolean
): void {
    wireRegistry.push({ fromEl, toEl, color, dimmed });
}

export function updateWireDimming(query: string): void {
    for (const w of wireRegistry) {
        const name = (w.toEl as any)._itemName as string ?? '';
        w.dimmed = !!query && !name.toLowerCase().includes(query.toLowerCase());
    }
}

export function redrawWires(): void {
    const c   = wiresCanvas();
    const ctx = c.getContext('2d')!;
    ctx.clearRect(0, 0, c.width, c.height);

    const nodeWidth = 200;

    for (const w of wireRegistry) {
        const x1 = parseFloat(w.fromEl.style.left) + nodeWidth;
        const y1 = parseFloat(w.fromEl.style.top)  + 40;
        const x2 = parseFloat(w.toEl.style.left);
        const y2 = parseFloat(w.toEl.style.top)    + 40;

        const dist = Math.abs(x2 - x1) * 0.5;
        ctx.beginPath();
        ctx.moveTo(x1, y1);
        ctx.bezierCurveTo(x1 + dist, y1, x2 - dist, y2, x2, y2);
        ctx.strokeStyle  = w.color;
        ctx.globalAlpha  = w.dimmed ? 0.1 : 0.4;
        ctx.lineWidth    = 1.5;
        ctx.stroke();
    }
    ctx.globalAlpha = 1;
}
