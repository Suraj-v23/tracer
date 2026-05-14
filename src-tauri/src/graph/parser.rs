use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

// ─── Regex patterns (compiled once) ─────────────────────────────────────────

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("invalid regex")
}

static TS_IMPORT:       OnceLock<Regex> = OnceLock::new();
static JS_REQUIRE:      OnceLock<Regex> = OnceLock::new();
static PY_FROM:         OnceLock<Regex> = OnceLock::new();
static PY_IMPORT:       OnceLock<Regex> = OnceLock::new();
static RS_MOD:          OnceLock<Regex> = OnceLock::new();
static RS_USE:          OnceLock<Regex> = OnceLock::new();
static GO_IMPORT:       OnceLock<Regex> = OnceLock::new();
static GO_IMPORT_BLOCK: OnceLock<Regex> = OnceLock::new();
static CSS_IMPORT:      OnceLock<Regex> = OnceLock::new();
static HTML_LINK:       OnceLock<Regex> = OnceLock::new();
static HTML_SCRIPT:     OnceLock<Regex> = OnceLock::new();

// ─── Public API ───────────────────────────────────────────────────────────────

pub fn extract_imports(text: &str, extension: &str) -> Vec<String> {
    match extension {
        ".ts" | ".tsx" | ".js" | ".jsx" | ".mjs" => extract_js(text),
        ".py"                                     => extract_py(text),
        ".rs"                                     => extract_rs(text),
        ".go"                                     => extract_go(text),
        ".css" | ".scss" | ".sass"                => extract_css(text),
        ".html" | ".htm"                          => extract_html(text),
        _                                         => vec![],
    }
}

pub fn resolve_import(raw: &str, source_file: &Path) -> Option<String> {
    if !raw.starts_with('.') {
        return None;
    }
    let dir = source_file.parent()?;
    let resolved = dir.join(raw);
    if resolved.exists() {
        return std::fs::canonicalize(&resolved).ok()
            .map(|p| p.to_string_lossy().to_string())
            .or_else(|| Some(resolved.to_string_lossy().to_string()));
    }
    for ext in &[".ts", ".tsx", ".js", ".jsx", ".py", ".rs"] {
        let with_ext = dir.join(format!("{raw}{ext}"));
        if with_ext.exists() {
            return std::fs::canonicalize(&with_ext).ok()
                .map(|p| p.to_string_lossy().to_string())
                .or_else(|| Some(with_ext.to_string_lossy().to_string()));
        }
    }
    for index in &["index.ts", "index.js", "mod.rs"] {
        let index_path = resolved.join(index);
        if index_path.exists() {
            return Some(index_path.to_string_lossy().to_string());
        }
    }
    None
}

// ─── Per-language extractors ─────────────────────────────────────────────────

fn extract_js(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let ts = TS_IMPORT.get_or_init(|| re(r#"(?m)import\s+(?:[^'"]*\s+from\s+)?['"]([^'"]+)['"]"#));
    for cap in ts.captures_iter(text) { out.push(cap[1].to_string()); }
    let req = JS_REQUIRE.get_or_init(|| re(r#"require\s*\(\s*['"]([^'"]+)['"]\s*\)"#));
    for cap in req.captures_iter(text) { out.push(cap[1].to_string()); }
    out
}

fn extract_py(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let from = PY_FROM.get_or_init(|| re(r"(?m)^from\s+(\.\.?[\w./]*)\s+import"));
    for cap in from.captures_iter(text) {
        let raw = &cap[1];
        let path = raw.replacen('.', "./", 1).replace('.', "/");
        out.push(path);
    }
    let imp = PY_IMPORT.get_or_init(|| re(r"(?m)^import\s+([\w.]+)"));
    for cap in imp.captures_iter(text) {
        let raw = cap[1].to_string();
        if raw.starts_with('.') { out.push(raw); }
    }
    out
}

fn extract_rs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mod_re = RS_MOD.get_or_init(|| re(r"(?m)^\s*(?:pub\s+)?mod\s+(\w+)\s*;"));
    for cap in mod_re.captures_iter(text) { out.push(format!("./{}", &cap[1])); }
    let use_re = RS_USE.get_or_init(|| re(r"(?m)^\s*use\s+((?:super|self)::[\w:]+)"));
    for cap in use_re.captures_iter(text) {
        let raw = cap[1].replace("::", "/").replace("super", "..");
        out.push(raw);
    }
    out
}

fn extract_go(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let re_single = GO_IMPORT.get_or_init(|| re(r#"import\s+"(\.[\w./]+)""#));
    for cap in re_single.captures_iter(text) { out.push(cap[1].to_string()); }
    let block = GO_IMPORT_BLOCK.get_or_init(|| re(r#""(\.[\w./]+)""#));
    for cap in block.captures_iter(text) { out.push(cap[1].to_string()); }
    out
}

fn extract_css(text: &str) -> Vec<String> {
    let re = CSS_IMPORT.get_or_init(|| re(r#"@import\s+['"]([^'"]+)['"]"#));
    re.captures_iter(text).map(|c| c[1].to_string()).collect()
}

fn extract_html(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let link = HTML_LINK.get_or_init(|| re(r#"<link[^>]+href=['"]([^'"]+\.css)['"]"#));
    for cap in link.captures_iter(text) { out.push(cap[1].to_string()); }
    let script = HTML_SCRIPT.get_or_init(|| re(r#"<script[^>]+src=['"]([^'"]+)['"]"#));
    for cap in script.captures_iter(text) { out.push(cap[1].to_string()); }
    out
}

// ─── Index imports ────────────────────────────────────────────────────────────

/// Walk all code files under `root`, parse their imports, resolve paths,
/// and insert `imports` edges into the store.
pub fn index_imports(root: &Path, store: &crate::graph::store::Store) {
    use walkdir::WalkDir;
    use rayon::prelude::*;

    let entries: Vec<_> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .collect();

    // Extract (source_path, Vec<resolved_target_path>) in parallel
    let pairs: Vec<(String, Vec<String>)> = entries.par_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let ext = path.extension()
                .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                .unwrap_or_default();
            let text = std::fs::read_to_string(path).ok()?;
            let raw_imports = extract_imports(&text, &ext);
            if raw_imports.is_empty() { return None; }
            let resolved: Vec<String> = raw_imports.iter()
                .filter_map(|raw| resolve_import(raw, path))
                .collect();
            if resolved.is_empty() { return None; }
            Some((path.to_string_lossy().to_string(), resolved))
        })
        .collect();

    // Write edges sequentially (SQLite single-writer)
    let _ = store.conn.execute("BEGIN", []);
    for (from_path, targets) in &pairs {
        for to_path in targets {
            let _ = store.upsert_edge(from_path, to_path, "imports");
        }
    }
    let _ = store.conn.execute("COMMIT", []);
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_import_from() {
        let src = "import { foo } from './utils'\nimport type { Bar } from '../types'\nimport 'side-effect'";
        let imports = extract_imports(src, ".ts");
        assert!(imports.contains(&"./utils".to_string()));
        assert!(imports.contains(&"../types".to_string()));
        assert!(imports.contains(&"side-effect".to_string()));
    }

    #[test]
    fn js_require() {
        let src = "const x = require('./lib')\nconst y = require(\"../config\")";
        let imports = extract_imports(src, ".js");
        assert!(imports.contains(&"./lib".to_string()));
        assert!(imports.contains(&"../config".to_string()));
    }

    #[test]
    fn py_relative_from() {
        let src = "from .models import User\nfrom ..utils import helper";
        let imports = extract_imports(src, ".py");
        assert_eq!(imports.len(), 2);
    }

    #[test]
    fn rs_mod_declaration() {
        let src = "pub mod store;\nmod query;\nuse std::io;";
        let imports = extract_imports(src, ".rs");
        assert!(imports.contains(&"./store".to_string()));
        assert!(imports.contains(&"./query".to_string()));
        assert!(!imports.iter().any(|s| s.contains("std")));
    }

    #[test]
    fn css_import() {
        let src = "@import './variables.css'\n@import \"../reset.css\"";
        let imports = extract_imports(src, ".css");
        assert_eq!(imports.len(), 2);
    }

    #[test]
    fn html_links_and_scripts() {
        let src = "<link rel=\"stylesheet\" href=\"./style.css\">\n<script src=\"../js/app.js\"></script>";
        let imports = extract_imports(src, ".html");
        assert!(imports.iter().any(|s| s.contains("style.css")));
        assert!(imports.iter().any(|s| s.contains("app.js")));
    }

    #[test]
    fn resolve_relative_path_adds_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("utils.ts"), b"export {}").unwrap();
        let source = dir.path().join("main.ts");
        let resolved = resolve_import("./utils", &source);
        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("utils.ts"));
    }

    #[test]
    fn resolve_bare_module_returns_none() {
        let source = std::path::Path::new("/project/src/main.ts");
        assert!(resolve_import("react", source).is_none());
        assert!(resolve_import("std::io", source).is_none());
    }

    #[test]
    fn unknown_extension_returns_empty() {
        let imports = extract_imports("whatever content", ".exe");
        assert!(imports.is_empty());
    }

    #[test]
    fn index_imports_for_folder() {
        use crate::graph::store::{GraphNode, Store};
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        // Canonicalize the root so walkdir paths and stored paths always match,
        // even on macOS where /var/folders may be a symlink to /private/var/folders.
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let main_path  = root.join("main.ts");
        let utils_path = root.join("utils.ts");
        fs::write(&main_path,  b"import { foo } from './utils'").unwrap();
        fs::write(&utils_path, b"export function foo() {}").unwrap();

        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&GraphNode { id: 0,
            path: main_path.to_string_lossy().to_string(),
            name: "main.ts".into(), kind: "file".into(), size: 28,
            extension: Some(".ts".into()), modified_secs: None, created_secs: None, content_hash: None,
        }).unwrap();
        store.upsert_node(&GraphNode { id: 0,
            path: utils_path.to_string_lossy().to_string(),
            name: "utils.ts".into(), kind: "file".into(), size: 24,
            extension: Some(".ts".into()), modified_secs: None, created_secs: None, content_hash: None,
        }).unwrap();

        index_imports(&root, &store);

        let imports = store.get_imports(&main_path.to_string_lossy()).unwrap();
        assert_eq!(imports.len(), 1, "main.ts should import utils.ts");
        assert_eq!(imports[0].name, "utils.ts");
    }
}
