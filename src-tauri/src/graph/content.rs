use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::graph::store::Store;

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024; // 2 MB

const TEXT_EXTENSIONS: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx", ".mjs",
    ".py", ".rs", ".go", ".java", ".c", ".cpp", ".h", ".hpp",
    ".md", ".txt", ".rst",
    ".json", ".yaml", ".yml", ".toml", ".xml",
    ".html", ".htm", ".css", ".scss", ".sass",
    ".sh", ".bash", ".zsh", ".fish",
    ".sql", ".graphql", ".proto",
    ".env", ".gitignore", ".dockerfile",
];

/// Returns extracted text if the file is indexable, None otherwise.
pub fn extract_text_from_bytes(bytes: &[u8], extension: &str) -> Option<String> {
    let ext = extension.to_lowercase();

    if !TEXT_EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }

    if bytes.len() > MAX_FILE_BYTES as usize {
        return None;
    }

    // Heuristic binary detection: if >5% non-printable bytes in first 512 bytes, skip
    let sample = &bytes[..bytes.len().min(512)];
    let non_utf8 = sample.iter().filter(|&&b| b < 9 || (b > 13 && b < 32 && b != 27)).count();
    if non_utf8 > sample.len() / 20 {
        return None;
    }

    String::from_utf8(bytes.to_vec())
        .ok()
        .map(|s| s.chars().take(50_000).collect())
}

/// Index all text files under `root` that are already present in the node store.
/// Returns count of files successfully indexed.
pub fn index_folder(root: &Path, store: &Store) -> usize {
    let entries: Vec<_> = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.metadata().map(|m| m.len() <= MAX_FILE_BYTES).unwrap_or(false)
        })
        .collect();

    let count = Arc::new(AtomicUsize::new(0));

    // Parallel text extraction — pure CPU/IO, no store access
    let extracted: Vec<(String, String)> = entries.par_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let ext = path.extension()
                .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                .unwrap_or_default();
            let bytes = std::fs::read(path).ok()?;
            let text = extract_text_from_bytes(&bytes, &ext)?;
            Some((path.to_string_lossy().to_string(), text))
        })
        .collect();

    // Sequential store writes
    let _ = store.conn.execute("BEGIN", []);
    for (path, text) in &extracted {
        if let Ok(Some(node_id)) = store.get_node_id(path) {
            if store.index_content(node_id, text).is_ok() {
                count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    let _ = store.conn.execute("COMMIT", []);

    count.load(Ordering::Relaxed)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_from_ts_file() {
        let text = extract_text_from_bytes(b"export function hello() { return 42; }", ".ts");
        assert!(text.is_some());
        assert!(text.unwrap().contains("hello"));
    }

    #[test]
    fn skips_binary_content() {
        let binary = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
        let text = extract_text_from_bytes(binary, ".png");
        assert!(text.is_none());
    }

    #[test]
    fn skips_unsupported_extension() {
        let text = extract_text_from_bytes(b"some content", ".exe");
        assert!(text.is_none());
    }

    #[test]
    fn respects_size_limit() {
        let big = vec![b'a'; 3 * 1024 * 1024];
        let text = extract_text_from_bytes(&big, ".txt");
        assert!(text.is_none());
    }

    #[test]
    fn indexes_folder_scans_files() {
        use crate::graph::store::{GraphNode, Store};
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hello.ts"), b"export function greet() {}").unwrap();
        fs::write(dir.path().join("notes.md"), b"# Notes\nThis is about authentication").unwrap();
        fs::write(dir.path().join("binary.png"), b"\x89PNG\r\n\x1a\n").unwrap();

        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&GraphNode {
            id: 0, path: dir.path().join("hello.ts").to_string_lossy().to_string(),
            name: "hello.ts".into(), kind: "file".into(), size: 26,
            extension: Some(".ts".into()), modified_secs: None, created_secs: None, content_hash: None,
        }).unwrap();
        store.upsert_node(&GraphNode {
            id: 0, path: dir.path().join("notes.md").to_string_lossy().to_string(),
            name: "notes.md".into(), kind: "file".into(), size: 37,
            extension: Some(".md".into()), modified_secs: None, created_secs: None, content_hash: None,
        }).unwrap();

        let count = index_folder(dir.path(), &store);
        assert_eq!(count, 2);

        let results = store.content_search("authentication").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "notes.md");
    }
}
