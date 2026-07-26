use std::collections::BTreeSet;

use crate::domain::{KnowledgeChunk, SemanticProviderInfo};
use crate::error::DomainError;
use crate::ports::{SemanticExtractRequest, SemanticExtractResponse, SemanticExtractor};
use crate::usecases::fnv1a64;

/// 把文本按空行切分为段落，返回每个段落在原文中的字节区间 `(start, end)`。
///
/// - 空行判定：整行 trim（含 `'\r'` 与其他空白）后为空，因此同时兼容
///   LF（`\n\n`）与 Windows CRLF（`\r\n\r\n`）的段落分隔，也容忍连续多个空行。
/// - 返回的区间是指向原文的精确字节偏移，且已去除段落首尾空白，
///   可直接用于 `&text[start..end]` 切片与 chunk 的 `byte_range`。
pub fn split_paragraph_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    let mut offset = 0usize;

    for line in text.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content = content.strip_suffix('\r').unwrap_or(content);
        let trimmed = content.trim();
        if trimmed.is_empty() {
            if let Some(span) = current.take() {
                spans.push(span);
            }
        } else {
            let leading = content.len() - content.trim_start().len();
            let start = offset + leading;
            let end = start + trimmed.len();
            current = match current {
                Some((span_start, _)) => Some((span_start, end)),
                None => Some((start, end)),
            };
        }
        offset += line.len();
    }
    if let Some(span) = current {
        spans.push(span);
    }
    spans
}

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
        for (start, end) in split_paragraph_spans(&request.text).into_iter().take(8) {
            let chunk_text = &request.text[start..end];
            let chunk_index = chunks.len() + 1;
            chunks.push(KnowledgeChunk {
                chunk_id: format!("{}:{:04}", request.record_id, chunk_index),
                byte_range: [start as u64, end as u64],
                content_signature: format!("fnv1a64:{:016x}", fnv1a64(chunk_text.as_bytes())),
                summary: stable_truncate(chunk_text, 120),
                preview: stable_truncate(chunk_text, 200),
            });
        }
        if chunks.is_empty() {
            chunks.push(KnowledgeChunk {
                chunk_id: format!("{}:0001", request.record_id),
                byte_range: [0, request.text.len() as u64],
                content_signature: format!("fnv1a64:{:016x}", fnv1a64(request.text.as_bytes())),
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

#[cfg(test)]
mod tests {
    use super::{split_paragraph_spans, RuleBasedSemanticExtractor};
    use crate::ports::{SemanticExtractRequest, SemanticExtractor};
    use crate::usecases::fnv1a64;

    fn request(text: &str) -> SemanticExtractRequest {
        SemanticExtractRequest {
            record_id: "docs/a".to_string(),
            path: "./docs/a.md".to_string(),
            text: text.to_string(),
            input_signature: "sig".to_string(),
        }
    }

    #[test]
    fn split_paragraph_spans_handles_lf_paragraphs() {
        let text = "Para one.\n\nPara two.\n";
        assert_eq!(split_paragraph_spans(text), vec![(0, 9), (11, 20)]);
    }

    #[test]
    fn split_paragraph_spans_handles_crlf_paragraphs() {
        let text = "Para one.\r\n\r\nPara two.\r\n";
        let spans = split_paragraph_spans(text);
        assert_eq!(spans, vec![(0, 9), (13, 22)]);
        assert_eq!(&text[spans[0].0..spans[0].1], "Para one.");
        assert_eq!(&text[spans[1].0..spans[1].1], "Para two.");
    }

    #[test]
    fn split_paragraph_spans_tolerates_extra_blank_lines_and_indent() {
        let text = "a\n\n\n  b\nc  \n\n";
        let spans = split_paragraph_spans(text);
        assert_eq!(spans, vec![(0, 1), (6, 9)]);
        assert_eq!(&text[spans[1].0..spans[1].1], "b\nc");
    }

    #[test]
    fn split_paragraph_spans_returns_empty_for_whitespace_only() {
        assert!(split_paragraph_spans("").is_empty());
        assert!(split_paragraph_spans(" \r\n\t\n").is_empty());
    }

    #[test]
    fn rule_extractor_chunks_crlf_text_with_real_offsets() {
        let text = "Para one.\r\n\r\nPara two.\r\n";
        let response = RuleBasedSemanticExtractor::new()
            .extract(&request(text))
            .expect("extract should succeed");

        assert_eq!(response.chunks.len(), 2);
        assert_eq!(response.chunks[0].chunk_id, "docs/a:0001");
        assert_eq!(response.chunks[0].byte_range, [0, 9]);
        assert_eq!(
            response.chunks[0].content_signature,
            format!("fnv1a64:{:016x}", fnv1a64("Para one.".as_bytes()))
        );
        assert_eq!(response.chunks[1].chunk_id, "docs/a:0002");
        assert_eq!(response.chunks[1].byte_range, [13, 22]);
        assert_eq!(
            response.chunks[1].content_signature,
            format!("fnv1a64:{:016x}", fnv1a64("Para two.".as_bytes()))
        );
        assert_eq!(response.chunks[1].preview, "Para two.");
    }

    #[test]
    fn rule_extractor_caps_chunks_at_eight() {
        let text = (1..=12)
            .map(|idx| format!("Paragraph {idx}."))
            .collect::<Vec<_>>()
            .join("\n\n");
        let response = RuleBasedSemanticExtractor::new()
            .extract(&request(&text))
            .expect("extract should succeed");
        assert_eq!(response.chunks.len(), 8);
        assert_eq!(response.chunks[7].chunk_id, "docs/a:0008");
    }

    #[test]
    fn rule_extractor_falls_back_to_whole_text_chunk() {
        let text = "   \r\n\r\n  ";
        let response = RuleBasedSemanticExtractor::new()
            .extract(&request(text))
            .expect("extract should succeed");
        assert_eq!(response.chunks.len(), 1);
        assert_eq!(response.chunks[0].chunk_id, "docs/a:0001");
        assert_eq!(response.chunks[0].byte_range, [0, text.len() as u64]);
        assert_eq!(
            response.chunks[0].content_signature,
            format!("fnv1a64:{:016x}", fnv1a64(text.as_bytes()))
        );
    }
}
