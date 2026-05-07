function _invoke(cmd, args) {
    const tauri = window.__TAURI_INTERNALS__;
    if (!tauri) {
        const msg = document.getElementById('loading');
        if (msg) {
            msg.innerHTML = '<div style="padding:40px;text-align:center;color:#fff">' +
                '<div style="font-size:2rem;margin-bottom:16px">⚠️</div>' +
                '<div>Run inside Tauri: <code>npm run dev</code></div></div>';
            msg.classList.remove('hidden');
        }
        throw new Error('Tauri runtime not available — open via npm run dev');
    }
    return tauri.invoke(cmd, args);
}
export async function getFilesystem(path, depth = 2, force = false) {
    return _invoke('get_filesystem', { path, depth, force });
}
export async function deleteItem(path) {
    return _invoke('delete_item', { path });
}
export async function getHomeDir() {
    return _invoke('get_home_dir');
}
