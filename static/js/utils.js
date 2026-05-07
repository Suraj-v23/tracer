export const TYPE_MAP = {
    image: ['.jpg', '.jpeg', '.png', '.gif', '.bmp', '.svg', '.webp', '.ico', '.heic'],
    video: ['.mp4', '.avi', '.mkv', '.mov', '.wmv', '.flv', '.webm'],
    audio: ['.mp3', '.wav', '.flac', '.aac', '.ogg', '.wma', '.m4a'],
    code: ['.js', '.ts', '.py', '.java', '.cpp', '.c', '.h', '.go', '.rs', '.rb', '.php',
        '.swift', '.kt', '.sh', '.bash', '.json', '.yaml', '.toml', '.xml', '.css', '.html'],
    doc: ['.pdf', '.doc', '.docx', '.txt', '.md', '.rtf', '.odt', '.xls', '.xlsx', '.ppt', '.pptx'],
    archive: ['.zip', '.rar', '.7z', '.tar', '.gz', '.bz2', '.xz', '.iso'],
};
export const TYPE_ICONS = {
    directory: '📁', image: '🖼', video: '🎬', audio: '🎵',
    code: '💻', doc: '📄', archive: '📦', other: '📎',
};
export const COLORS = {
    folder: '#6b9fd4',
    image: '#c47aa0',
    video: '#c47d5a',
    audio: '#5a9e7a',
    code: '#5aadd4',
    doc: '#c4a84f',
    archive: '#8b78c4',
    other: '#6b7280',
};
export function formatSize(bytes) {
    if (bytes === 0)
        return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
    let size = bytes;
    let idx = 0;
    while (size >= 1000 && idx < units.length - 1) {
        size /= 1000;
        idx++;
    }
    return idx === 0 ? `${bytes} B` : `${size.toFixed(1)} ${units[idx]}`;
}
export function getFileCategory(item) {
    if (item.type === 'directory')
        return 'directory';
    const ext = (item.extension ?? '').toLowerCase();
    for (const [cat, exts] of Object.entries(TYPE_MAP)) {
        if (exts.includes(ext))
            return cat;
    }
    return 'other';
}
export function getColor(item) {
    const cat = getFileCategory(item);
    return cat === 'directory' ? COLORS.folder : (COLORS[cat] ?? COLORS.other);
}
