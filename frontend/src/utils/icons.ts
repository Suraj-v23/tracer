// ─── Single source of truth for all icons ────────────────────────────────────
// Drop any SVG or PNG into frontend/public/icons/, then reference it like:
//   directory: img('folder.svg'),
// Or keep the default emoji by using the string directly:
//   directory: '📁',

function img(file: string, alt = ''): string {
    return `<img src="/icons/${file}" class="icon-img" alt="${alt}">`;
}

export const FILE_ICONS: Record<string, string> = {
    directory: '📁',
    image:     '🖼',
    video:     '🎬',
    audio:     '🎵',
    code:      '💻',
    doc:       '📄',
    archive:   '📦',
    other:     '📎',
};

export const UI_ICONS = {
    back:      '←',
    forward:   '→',
    search:    '⌕',
    close:     '✕',
    warning:   '⚠️',
    newFile:   '📄',
    newFolder: '📁',
} as const;
