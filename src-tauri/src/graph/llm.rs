use serde::{Deserialize, Serialize};
use crate::graph::query::StructuredQuery;

// ─── Config ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub base_url: String,
    pub model:    String,
    pub api_key:  Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            model:    "llama3.2".into(),
            api_key:  None,
        }
    }
}

// ─── NL → StructuredQuery ────────────────────────────────────────────────────

const SCHEMA_PROMPT: &str = r#"Convert the filesystem question to JSON. Return ONLY valid JSON, no markdown, no explanation.

Shapes:
{"kind":"MetadataFilter","name_contains":"...","extension":".mp4","kind_filter":"file","size_gt":104857600,"size_lt":null,"modified_after":null}
{"kind":"FindDuplicates","path":"/"}
{"kind":"GetRelated","path":"/some/dir","depth":1}

Rules:
- size_gt / size_lt: bytes as integer or null
- modified_after: unix timestamp integer or null
- kind_filter: "file" or "directory" or null
- extension: include the dot, e.g. ".mp4" not "mp4"
- For "duplicate" questions use FindDuplicates with path "/"
- Null fields can be omitted

Question: "#;

pub async fn nl_to_query(question: &str, config: &LlmConfig) -> Result<StructuredQuery, String> {
    let prompt = format!("{SCHEMA_PROMPT}{question}");
    let raw = call_llm_raw(config, &prompt).await?;

    let cleaned = raw.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    serde_json::from_str::<StructuredQuery>(cleaned)
        .map_err(|e| format!("LLM returned invalid JSON: {e}\nRaw: {raw}"))
}

pub async fn call_llm_raw(config: &LlmConfig, prompt: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    match config.provider.as_str() {
        "ollama" => {
            let url  = format!("{}/api/generate", config.base_url);
            let body = serde_json::json!({ "model": config.model, "prompt": prompt, "stream": false });
            let resp = client.post(&url).json(&body).send().await
                .map_err(|e| format!("Ollama unreachable: {e}"))?
                .json::<serde_json::Value>().await
                .map_err(|e| format!("Ollama response parse failed: {e}"))?;
            resp["response"].as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| "Ollama response missing 'response' field".into())
        }
        "remote" => {
            let url     = format!("{}/chat/completions", config.base_url);
            let api_key = config.api_key.as_deref().unwrap_or("");
            let body = serde_json::json!({
                "model": config.model,
                "messages": [{ "role": "user", "content": prompt }],
                "temperature": 0
            });
            let resp = client.post(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .json(&body)
                .send().await
                .map_err(|e| format!("Remote LLM unreachable: {e}"))?
                .json::<serde_json::Value>().await
                .map_err(|e| format!("Remote LLM response parse failed: {e}"))?;
            resp["choices"][0]["message"]["content"].as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| "Remote LLM response missing content".into())
        }
        other => Err(format!("Unknown LLM provider: '{other}'. Use 'ollama' or 'remote'.")),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::query::StructuredQuery;

    fn parse(json: &str) -> Result<StructuredQuery, String> {
        serde_json::from_str::<StructuredQuery>(json)
            .map_err(|e| format!("parse failed: {e}"))
    }

    #[test]
    fn parses_metadata_filter() {
        let q = parse(r#"{"kind":"MetadataFilter","extension":".mp4","size_gt":104857600}"#).unwrap();
        assert!(matches!(q, StructuredQuery::MetadataFilter { .. }));
    }

    #[test]
    fn parses_find_duplicates() {
        let q = parse(r#"{"kind":"FindDuplicates","path":"/"}"#).unwrap();
        assert!(matches!(q, StructuredQuery::FindDuplicates { .. }));
    }

    #[test]
    fn parses_get_related() {
        let q = parse(r#"{"kind":"GetRelated","path":"/home","depth":2}"#).unwrap();
        match q {
            StructuredQuery::GetRelated { path, depth } => {
                assert_eq!(path, "/home");
                assert_eq!(depth, 2);
            }
            _ => panic!("expected GetRelated"),
        }
    }

    #[test]
    fn strips_markdown_fences() {
        let raw = "```json\n{\"kind\":\"FindDuplicates\",\"path\":\"/\"}\n```";
        let cleaned = raw.trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let q = parse(cleaned).unwrap();
        assert!(matches!(q, StructuredQuery::FindDuplicates { .. }));
    }

    #[test]
    fn unknown_provider_error_message() {
        let config = LlmConfig { provider: "unknown".into(), ..LlmConfig::default() };
        assert_eq!(config.provider, "unknown");
    }
}
