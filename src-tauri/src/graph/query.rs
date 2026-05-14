use serde::{Deserialize, Serialize};
use crate::graph::store::{Store, SearchResult};

// ─── Query types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum StructuredQuery {
    MetadataFilter {
        #[serde(default)] name_contains:  Option<String>,
        #[serde(default)] extension:      Option<String>,
        #[serde(default)] kind_filter:    Option<String>,
        #[serde(default)] size_gt:        Option<u64>,
        #[serde(default)] size_lt:        Option<u64>,
        #[serde(default)] modified_after: Option<i64>,
    },
    FindDuplicates {
        path: String,
    },
    GetRelated {
        path:  String,
        #[serde(default = "default_depth")] depth: usize,
    },
    ContentSearch {
        terms: String,
    },
    GetImports {
        path: String,
    },
    GetImporters {
        path: String,
    },
    SemanticSearch {
        query: String,
        #[serde(default = "default_k")] k: usize,
    },
}

fn default_depth() -> usize { 1 }
fn default_k() -> usize { 10 }

impl Default for StructuredQuery {
    fn default() -> Self {
        StructuredQuery::MetadataFilter {
            name_contains: None, extension: None, kind_filter: None,
            size_gt: None, size_lt: None, modified_after: None,
        }
    }
}

// ─── Execution ───────────────────────────────────────────────────────────────

pub fn execute(query: &StructuredQuery, store: &Store) -> Result<Vec<SearchResult>, String> {
    match query {
        StructuredQuery::MetadataFilter {
            name_contains, extension, kind_filter, size_gt, size_lt, modified_after
        } => store.query_metadata(
            name_contains.as_deref(),
            extension.as_deref(),
            kind_filter.as_deref(),
            *size_gt, *size_lt, *modified_after,
        ).map_err(|e| e.to_string()),

        StructuredQuery::FindDuplicates { path } =>
            store.find_duplicates(path).map_err(|e| e.to_string()),

        StructuredQuery::GetRelated { path, depth } =>
            store.get_children(path, *depth).map_err(|e| e.to_string()),

        StructuredQuery::ContentSearch { terms } =>
            store.content_search(terms).map_err(|e| e.to_string()),

        StructuredQuery::GetImports { path } =>
            store.get_imports(path).map_err(|e| e.to_string()),

        StructuredQuery::GetImporters { path } =>
            store.get_importers(path).map_err(|e| e.to_string()),

        StructuredQuery::SemanticSearch { .. } => Ok(vec![]),
    }
}

// ─── NL fallback (no LLM) ────────────────────────────────────────────────────

/// Best-effort heuristic parse when LLM is unavailable.
pub fn heuristic_parse(input: &str) -> StructuredQuery {
    let lower = input.to_lowercase();

    if lower.contains("duplicate") || lower.contains("dupe") {
        return StructuredQuery::FindDuplicates { path: "/".into() };
    }

    // Content search keywords
    if lower.contains("containing") || lower.contains("content") || lower.contains("inside")
        || lower.contains("with text") || lower.contains("mentions")
    {
        let skip = ["find","files","containing","content","inside","with","text","mentions","that","show","me"];
        let terms = input
            .split_whitespace()
            .filter(|w| !skip.contains(&w.to_lowercase().as_str()))
            .collect::<Vec<_>>()
            .join(" ");
        return StructuredQuery::ContentSearch {
            terms: if terms.is_empty() { input.into() } else { terms },
        };
    }

    let extension = if lower.contains("video") || lower.contains(".mp4") { Some(".mp4".into()) }
        else if lower.contains("image") || lower.contains("photo") { Some(".jpg".into()) }
        else if lower.contains("pdf") { Some(".pdf".into()) }
        else { None };

    let size_gt = if lower.contains("large") || lower.contains("big") { Some(100 * 1024 * 1024) }
        else { None };

    StructuredQuery::MetadataFilter {
        name_contains: Some(input.into()),
        extension,
        kind_filter: None,
        size_gt,
        size_lt: None,
        modified_after: None,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::store::Store;

    fn seed_store() -> Store {
        let store = Store::open_in_memory().unwrap();
        let nodes = vec![
            ("file", "/home/a.mp4",  "a.mp4",  ".mp4",  200_000_000_i64),
            ("file", "/home/b.txt",  "b.txt",  ".txt",  1_000),
            ("file", "/home/c.rs",   "c.rs",   ".rs",   5_000),
            ("directory", "/home/src", "src",  "",      0),
        ];
        for (kind, path, name, ext, size) in nodes {
            store.conn.execute(
                "INSERT INTO nodes (path,name,kind,size,extension) VALUES (?1,?2,?3,?4,?5)",
                rusqlite::params![path, name, kind, size, if ext.is_empty() { None } else { Some(ext) }],
            ).unwrap();
        }
        store
    }

    #[test]
    fn metadata_filter_by_extension() {
        let store = seed_store();
        let q = StructuredQuery::MetadataFilter {
            name_contains: None, extension: Some(".mp4".into()),
            kind_filter: None, size_gt: None, size_lt: None, modified_after: None,
        };
        let results = execute(&q, &store).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "a.mp4");
    }

    #[test]
    fn metadata_filter_size_gt() {
        let store = seed_store();
        let q = StructuredQuery::MetadataFilter {
            name_contains: None, extension: None, kind_filter: None,
            size_gt: Some(100_000_000), size_lt: None, modified_after: None,
        };
        let results = execute(&q, &store).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "a.mp4");
    }

    #[test]
    fn heuristic_parse_detects_large_video() {
        let q = heuristic_parse("find large videos");
        match q {
            StructuredQuery::MetadataFilter { extension, size_gt, .. } => {
                assert_eq!(extension, Some(".mp4".into()));
                assert!(size_gt.is_some());
            }
            _ => panic!("expected MetadataFilter"),
        }
    }

    #[test]
    fn heuristic_parse_detects_duplicates() {
        let q = heuristic_parse("show duplicate files");
        assert!(matches!(q, StructuredQuery::FindDuplicates { .. }));
    }

    #[test]
    fn structured_query_roundtrips_json() {
        let q = StructuredQuery::MetadataFilter {
            name_contains: Some("readme".into()),
            extension: Some(".md".into()),
            kind_filter: None, size_gt: None, size_lt: None, modified_after: None,
        };
        let json = serde_json::to_string(&q).unwrap();
        let q2: StructuredQuery = serde_json::from_str(&json).unwrap();
        assert!(matches!(q2, StructuredQuery::MetadataFilter { .. }));
    }

    #[test]
    fn heuristic_parse_detects_content_search() {
        let q = heuristic_parse("find files containing TODO");
        assert!(matches!(q, StructuredQuery::ContentSearch { .. }));
    }

    #[test]
    fn content_search_roundtrips_json() {
        let q = StructuredQuery::ContentSearch { terms: "authentication".into() };
        let json = serde_json::to_string(&q).unwrap();
        let q2: StructuredQuery = serde_json::from_str(&json).unwrap();
        assert!(matches!(q2, StructuredQuery::ContentSearch { .. }));
    }
}
