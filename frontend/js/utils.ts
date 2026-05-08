export const TYPE_MAP: Record<string, string[]> = {
    image:   ['.jpg','.jpeg','.png','.gif','.bmp','.svg','.webp','.ico','.heic'],
    video:   ['.mp4','.avi','.mkv','.mov','.wmv','.flv','.webm'],
    audio:   ['.mp3','.wav','.flac','.aac','.ogg','.wma','.m4a'],
    code:    ['.js','.ts','.py','.java','.cpp','.c','.h','.go','.rs','.rb','.php',
              '.swift','.kt','.sh','.bash','.json','.yaml','.toml','.xml','.css','.html'],
    doc:     ['.pdf','.doc','.docx','.txt','.md','.rtf','.odt','.xls','.xlsx','.ppt','.pptx'],
    archive: ['.zip','.rar','.7z','.tar','.gz','.bz2','.xz','.iso'],
};

export const TYPE_ICONS: Record<string, string> = {
    directory: '📁', image: '🖼', video: '🎬', audio: '🎵',
    code: '💻', doc: '📄', archive: '📦', other: '📎',
};

export const COLORS: Record<string, string> = {
    folder:  '#6b9fd4',
    image:   '#c47aa0',
    video:   '#c47d5a',
    audio:   '#5a9e7a',
    code:    '#5aadd4',
    doc:     '#c4a84f',
    archive: '#8b78c4',
    other:   '#6b7280',
};

export function formatSize(bytes: number): string {
    if (bytes === 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
    let size = bytes;
    let idx = 0;
    while (size >= 1000 && idx < units.length - 1) { size /= 1000; idx++; }
    return idx === 0 ? `${bytes} B` : `${size.toFixed(1)} ${units[idx]}`;
}

export function getFileCategory(item: { type: string; extension?: string }): string {
    if (item.type === 'directory') return 'directory';
    const ext = (item.extension ?? '').toLowerCase();
    for (const [cat, exts] of Object.entries(TYPE_MAP)) {
        if (exts.includes(ext)) return cat;
    }
    return 'other';
}

export function getColor(item: { type: string; extension?: string }): string {
    const cat = getFileCategory(item);
    return cat === 'directory' ? COLORS.folder : (COLORS[cat] ?? COLORS.other);
}

// Maps file size to a hex colour: grey (tiny) → green → yellow → red (large).
// Returns hex so callers can append 2-digit alpha (e.g. color + '20').
export function sizeToColor(size: number, maxSize: number): string {
    if (maxSize === 0 || size === 0) return '#6b7280';
    const ratio = Math.log(size + 1) / Math.log(maxSize + 1); // 0..1
    if (ratio < 0.08) return '#6b7280';                        // very small → grey
    const t   = Math.min((ratio - 0.08) / 0.92, 1);           // 0..1
    const hue = Math.round(120 * (1 - t));                     // green(120°) → red(0°)
    return _hslToHex(hue, 62, 52);
}

function _hslToHex(h: number, s: number, l: number): string {
    s /= 100; l /= 100;
    const a = s * Math.min(l, 1 - l);
    const f = (n: number) => {
        const k = (n + h / 30) % 12;
        const c = l - a * Math.max(Math.min(k - 3, 9 - k, 1), -1);
        return Math.round(255 * c).toString(16).padStart(2, '0');
    };
    return `#${f(0)}${f(8)}${f(4)}`;
}
