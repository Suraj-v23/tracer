use std::collections::HashMap;
use regex::Regex;
use std::sync::OnceLock;

use crate::graph::store::{Store, CommunitySummary, SearchResult};
use crate::graph::llm::{LlmConfig, call_llm_raw};

// ─── Entity extraction ────────────────────────────────────────────────────────

static RE_JS_FN:     OnceLock<Regex> = OnceLock::new();
static RE_JS_CLASS:  OnceLock<Regex> = OnceLock::new();
static RE_PY_DEF:    OnceLock<Regex> = OnceLock::new();
static RE_PY_CLASS:  OnceLock<Regex> = OnceLock::new();
static RE_RS_FN:     OnceLock<Regex> = OnceLock::new();
static RE_RS_STRUCT: OnceLock<Regex> = OnceLock::new();
static RE_GO_FN:     OnceLock<Regex> = OnceLock::new();

fn re(p: &str) -> Regex { Regex::new(p).expect("bad regex") }

pub fn extract_code_entities(text: &str, extension: &str) -> Vec<(String, String)> {
    match extension {
        ".ts" | ".tsx" | ".js" | ".jsx" | ".mjs" => {
            let mut out = Vec::new();
            let fn_re  = RE_JS_FN.get_or_init(||    re(r"(?m)(?:export\s+)?(?:async\s+)?function\s+(\w+)"));
            let cls_re = RE_JS_CLASS.get_or_init(|| re(r"(?m)(?:export\s+)?class\s+(\w+)"));
            for cap in fn_re.captures_iter(text)  { out.push((cap[1].to_string(), "function".into())); }
            for cap in cls_re.captures_iter(text) { out.push((cap[1].to_string(), "class".into())); }
            out
        }
        ".py" => {
            let mut out = Vec::new();
            let def_re = RE_PY_DEF.get_or_init(||   re(r"(?m)^def\s+(\w+)"));
            let cls_re = RE_PY_CLASS.get_or_init(|| re(r"(?m)^class\s+(\w+)"));
            for cap in def_re.captures_iter(text) { out.push((cap[1].to_string(), "function".into())); }
            for cap in cls_re.captures_iter(text) { out.push((cap[1].to_string(), "class".into())); }
            out
        }
        ".rs" => {
            let mut out = Vec::new();
            let fn_re  = RE_RS_FN.get_or_init(||     re(r"(?m)pub\s+(?:async\s+)?fn\s+(\w+)"));
            let st_re  = RE_RS_STRUCT.get_or_init(|| re(r"(?m)pub\s+struct\s+(\w+)"));
            for cap in fn_re.captures_iter(text) { out.push((cap[1].to_string(), "function".into())); }
            for cap in st_re.captures_iter(text) { out.push((cap[1].to_string(), "struct".into())); }
            out
        }
        ".go" => {
            let mut out = Vec::new();
            let fn_re = RE_GO_FN.get_or_init(|| re(r"(?m)^func\s+(?:\(\w+\s+\*?\w+\)\s+)?(\w+)"));
            for cap in fn_re.captures_iter(text) { out.push((cap[1].to_string(), "function".into())); }
            out
        }
        _ => vec![],
    }
}

// ─── Entity indexing ──────────────────────────────────────────────────────────

pub fn index_all_entities(store: &Store) {
    let all: Vec<(i64, String, String)> = {
        let mut stmt = match store.conn.prepare(r#"
            SELECT m.node_id, n.extension, f.content
            FROM fts_node_map m
            JOIN nodes n ON n.id = m.node_id
            JOIN fts_content f ON f.rowid = m.rowid
        "#) {
            Ok(s) => s,
            Err(_) => return,
        };
        let rows = match stmt.query_map([], |r| Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            r.get::<_, String>(2)?,
        ))) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect::<Vec<_>>(),
            Err(_) => return,
        };
        rows
    };

    let _ = store.conn.execute("BEGIN", []);
    for (node_id, ext, content) in &all {
        let entities = extract_code_entities(content, ext);
        if !entities.is_empty() {
            let _ = store.insert_entities(*node_id, &entities);
        }
    }
    let _ = store.conn.execute("COMMIT", []);
}

// ─── Community detection (label propagation) ──────────────────────────────────

pub fn label_propagation(node_entities: &HashMap<i64, Vec<String>>) -> HashMap<i64, usize> {
    let mut entity_to_nodes: HashMap<&str, Vec<i64>> = HashMap::new();
    for (node_id, entities) in node_entities {
        for entity in entities {
            entity_to_nodes.entry(entity.as_str()).or_default().push(*node_id);
        }
    }

    let mut adjacency: HashMap<i64, HashMap<i64, f32>> = HashMap::new();
    for nodes in entity_to_nodes.values().filter(|v| v.len() > 1 && v.len() < 50) {
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                *adjacency.entry(nodes[i]).or_default().entry(nodes[j]).or_insert(0.0) += 1.0;
                *adjacency.entry(nodes[j]).or_default().entry(nodes[i]).or_insert(0.0) += 1.0;
            }
        }
    }

    let mut labels: HashMap<i64, usize> = node_entities.keys()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    let mut all_nodes: Vec<i64> = node_entities.keys().copied().collect();
    all_nodes.sort_unstable();

    for _ in 0..20 {
        let mut changed = false;
        for &node in &all_nodes {
            if let Some(neighbors) = adjacency.get(&node) {
                let mut votes: HashMap<usize, f32> = HashMap::new();
                for (&neighbor, &weight) in neighbors {
                    *votes.entry(labels[&neighbor]).or_insert(0.0) += weight;
                }
                if let Some((&best, _)) = votes.iter().max_by(|a, b| {
                    a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal)
                }) {
                    if labels[&node] != best {
                        labels.insert(node, best);
                        changed = true;
                    }
                }
            }
        }
        if !changed { break; }
    }

    let mut unique: Vec<usize> = labels.values().copied()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    unique.sort_unstable();
    let remap: HashMap<usize, usize> = unique.iter().enumerate()
        .map(|(i, &l)| (l, i))
        .collect();
    labels.into_iter().map(|(node, label)| (node, remap[&label])).collect()
}

pub fn rebuild_communities(store: &Store) {
    index_all_entities(store);

    let node_entities_raw = match store.get_all_node_entities() {
        Ok(v) => v,
        Err(_) => return,
    };
    if node_entities_raw.is_empty() { return; }

    let node_entities: HashMap<i64, Vec<String>> = node_entities_raw.into_iter().collect();
    let labels = label_propagation(&node_entities);

    let mut by_label: HashMap<usize, Vec<i64>> = HashMap::new();
    for (node_id, label) in &labels {
        by_label.entry(*label).or_default().push(*node_id);
    }

    let _ = store.clear_communities();
    let _ = store.conn.execute("BEGIN", []);
    for (_, members) in &by_label {
        if members.len() >= 2 {
            let _ = store.upsert_community(None, None, members);
        }
    }
    let _ = store.conn.execute("COMMIT", []);
}

// ─── Community summarization ──────────────────────────────────────────────────

pub async fn summarize_communities(store: &Store, config: &LlmConfig) {
    let communities = match store.list_communities() {
        Ok(c) => c,
        Err(_) => return,
    };

    for community in &communities {
        if community.summary.is_some() { continue; }

        let members = match store.get_community_members(community.id) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let file_list = members.iter().take(10)
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        let ids: Vec<i64> = serde_json::from_str(&community.member_ids).unwrap_or_default();
        let mut entity_set: std::collections::HashSet<String> = std::collections::HashSet::new();
        for &id in ids.iter().take(5) {
            if let Ok(names) = store.get_entity_names_for_node(id) {
                entity_set.extend(names.into_iter().take(5));
            }
        }
        let entity_names: Vec<String> = entity_set.into_iter().take(15).collect();

        let prompt = format!(
            "In one sentence, summarize what this group of code files is about.\nFiles: {}\nKey symbols: {}\nSummary:",
            file_list,
            entity_names.join(", ")
        );

        match call_llm_raw(config, &prompt).await {
            Ok(s) => { let _ = store.update_community_summary(community.id, s.trim()); }
            Err(e) => eprintln!("[community] summarize failed for {}: {e}", community.id),
        }
    }
}

// ─── GraphRAG global query ────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GlobalAnswer {
    pub answer:           String,
    pub sources:          Vec<SearchResult>,
    pub communities_used: Vec<i64>,
}

pub async fn global_query(
    question: &str,
    store: &Store,
    config: &LlmConfig,
) -> Result<GlobalAnswer, String> {
    let communities = store.list_communities().map_err(|e| e.to_string())?;
    if communities.is_empty() {
        return Err("No communities built yet. Run graph_rebuild_communities first.".to_string());
    }

    let summarized: Vec<&CommunitySummary> = communities.iter()
        .filter(|c| c.summary.is_some())
        .collect();

    let relevant_ids: Vec<i64> = if summarized.is_empty() {
        communities.iter().map(|c| c.id).take(3).collect()
    } else {
        let summaries_ctx = summarized.iter()
            .map(|c| format!("ID {}: {}", c.id, c.summary.as_deref().unwrap_or("")))
            .collect::<Vec<_>>()
            .join("\n");

        let map_prompt = format!(
            "These are summaries of code communities:\n{}\n\nWhich community IDs are most relevant to answer: \"{}\"\nReply with ONLY comma-separated numbers, e.g.: 1,3",
            summaries_ctx, question
        );

        match call_llm_raw(config, &map_prompt).await {
            Ok(raw) => raw.split(',').filter_map(|s| s.trim().parse().ok()).collect(),
            Err(_)  => communities.iter().map(|c| c.id).take(3).collect(),
        }
    };

    let member_node_ids: Vec<i64> = relevant_ids.iter()
        .filter_map(|&id| {
            communities.iter().find(|c| c.id == id)
                .and_then(|c| serde_json::from_str::<Vec<i64>>(&c.member_ids).ok())
        })
        .flatten()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let sources = store.get_nodes_by_ids(&member_node_ids).map_err(|e| e.to_string())?;

    let context = sources.iter().take(8)
        .map(|r| format!("- {}", r.name))
        .collect::<Vec<_>>()
        .join("\n");

    let answer_prompt = format!(
        "You are analyzing a codebase. Based on these relevant files:\n{}\n\nAnswer this question: {}\n\nBe concise and specific.",
        context, question
    );

    let answer = call_llm_raw(config, &answer_prompt).await
        .unwrap_or_else(|_| "Unable to generate answer — LLM unavailable.".to_string());

    Ok(GlobalAnswer {
        answer,
        sources: sources.into_iter().take(10).collect(),
        communities_used: relevant_ids,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_ts_entities() {
        let src = "export function authenticate(user: User) {}\nexport class TokenService {}";
        let entities = extract_code_entities(src, ".ts");
        let names: Vec<&str> = entities.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"authenticate"));
        assert!(names.contains(&"TokenService"));
    }

    #[test]
    fn extract_rs_entities() {
        let src = "pub fn process_request() {}\npub struct Handler {}";
        let entities = extract_code_entities(src, ".rs");
        let names: Vec<&str> = entities.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"process_request"));
        assert!(names.contains(&"Handler"));
    }

    #[test]
    fn label_propagation_clusters_related_nodes() {
        let mut node_entities: HashMap<i64, Vec<String>> = HashMap::new();
        node_entities.insert(1, vec!["auth".into(), "login".into()]);
        node_entities.insert(2, vec!["auth".into(), "logout".into()]);
        node_entities.insert(3, vec!["auth".into(), "token".into()]);
        node_entities.insert(4, vec!["payment".into(), "stripe".into()]);
        node_entities.insert(5, vec!["payment".into(), "invoice".into()]);

        let labels = label_propagation(&node_entities);
        assert_eq!(labels[&1], labels[&2]);
        assert_eq!(labels[&2], labels[&3]);
        assert_eq!(labels[&4], labels[&5]);
        assert_ne!(labels[&1], labels[&4]);
    }

    #[test]
    fn unknown_extension_returns_empty() {
        let entities = extract_code_entities("some content", ".exe");
        assert!(entities.is_empty());
    }
}
