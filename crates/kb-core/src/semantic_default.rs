use std::collections::BTreeSet;

use crate::domain::{KnowledgeChunk, SemanticProviderInfo};
use crate::error::DomainError;
use crate::ports::{SemanticExtractRequest, SemanticExtractResponse, SemanticExtractor};

#[derive(Debug, Default, Clone, Copy)]
pub struct RuleBasedSemanticExtractor;

impl RuleBasedSemanticExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl SemanticExtractor for RuleBasedSemanticExtractor {
    fn extract(
        &self,
        request: &SemanticExtractRequest,
    ) -> Result<SemanticExtractResponse, DomainError> {
        let summary = request
            .text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(|line| stable_truncate(line, 240))
            .unwrap_or_else(|| format!("Content from {}", request.path));

        let mut tags = BTreeSet::new();
        for token in request
            .path
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|token| !token.is_empty())
        {
            tags.insert(token.to_ascii_lowercase());
        }

        let mut key_concepts = request
            .text
            .lines()
            .map(str::trim)
            .filter_map(|line| line.strip_prefix('#'))
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string)
            .take(4)
            .collect::<Vec<_>>();
        if key_concepts.is_empty() {
            key_concepts = tags.iter().take(3).cloned().collect();
        }

        let mut chunks = Vec::new();
        for (idx, raw_chunk) in request.text.split("\n\n").enumerate().take(8) {
            let chunk_text = raw_chunk.trim();
            if chunk_text.is_empty() {
                continue;
            }
            chunks.push(KnowledgeChunk {
                chunk_id: format!("{}:{:04}", request.record_id, idx + 1),
                byte_range: [0, chunk_text.len() as u64],
                content_signature: format!("len:{}", chunk_text.len()),
                summary: stable_truncate(chunk_text, 120),
                preview: stable_truncate(chunk_text, 200),
            });
        }
        if chunks.is_empty() {
            chunks.push(KnowledgeChunk {
                chunk_id: format!("{}:0001", request.record_id),
                byte_range: [0, request.text.len() as u64],
                content_signature: format!("len:{}", request.text.len()),
                summary: summary.clone(),
                preview: stable_truncate(&request.text, 200),
            });
        }

        Ok(SemanticExtractResponse {
            summary,
            tags: tags.into_iter().take(8).collect(),
            key_concepts,
            chunks,
            provider: SemanticProviderInfo {
                kind: "rule".to_string(),
                name: "rule-based-v1".to_string(),
            },
        })
    }
}

fn stable_truncate(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    let mut out = text.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}
