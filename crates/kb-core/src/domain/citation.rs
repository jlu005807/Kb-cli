//! 引用校验（citation verification）：反幻觉的核心原语。
//!
//! LLM 给出的 citation（"我引用了 chunk X 的这段话"）默认是**不可信**的——
//! 模型可能复述了一段并不存在于该 chunk 的文字。`verify_citation` 在代码层做
//! **词法**校验：把声称的引用文字和候选证据文本（chunk 的 preview / 源文件切片）
//! 做归一化后比对，返回是否真的匹配。
//!
//! 刻意只做词法、不做语义：
//! - 语义相似度需要 embedding，违背"无 LLM 也能跑"的原则
//! - 反幻觉要的恰恰是"逐字可查"，语义近似反而会放过改写型幻觉
//!
//! 本模块是**纯逻辑**：输入 quote + 候选文本切片，输出裁决。读取 chunk /
//! 源文件由 CLI 层负责喂进来，保持 kb-core 零 IO。

use serde::{Deserialize, Serialize};

/// 校验方法：命中时说明用哪种匹配判定的，未命中时为 `None`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationMatch {
    /// 归一化空白后，引用是某段候选文本的连续子串（最强证据）。
    Substring,
    /// 引用的 token 在候选文本中的覆盖率达到阈值（容忍轻微改写 / 截断）。
    TokenOverlap,
    /// 未达到任何匹配标准。
    None,
}

/// 一次引用校验的裁决。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CitationVerdict {
    /// 是否判定为真实引用。
    pub matched: bool,
    /// 命中方法（未命中为 None）。
    pub method: CitationMatch,
    /// token 覆盖率 [0,1]，便于调用方按需设更严阈值或展示。
    pub coverage: f32,
    /// 命中的候选文本下标（substring / 最高覆盖率那条），未命中为 None。
    pub source_index: Option<usize>,
}

/// token 覆盖率达到此值即判 TokenOverlap 命中。0.8 = 允许约 1/5 的 token
/// 因截断 / 标点差异缺失，仍认定为同一段话。
const TOKEN_OVERLAP_THRESHOLD: f32 = 0.8;

/// 归一化：转小写 + 折叠所有空白为单空格 + 去首尾空白。
/// 用于子串比对，消除换行 / 缩进 / 全角空格带来的假阴性。
fn normalize_ws(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// 切分为去重 token 集合（按空白 + 小写）。用于覆盖率计算。
fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

/// 校验一条引用。
///
/// - `quote`：LLM 声称引用的文字。
/// - `candidates`：候选证据文本（chunk preview、源文件 byte_range 切片等）。
///
/// 判定顺序：先试归一化子串（最强），任一候选命中即返回 Substring；
/// 否则计算引用 token 在各候选中的最大覆盖率，达到阈值返回 TokenOverlap。
pub fn verify_citation(quote: &str, candidates: &[&str]) -> CitationVerdict {
    let quote_norm = normalize_ws(quote);
    if quote_norm.is_empty() {
        return CitationVerdict {
            matched: false,
            method: CitationMatch::None,
            coverage: 0.0,
            source_index: None,
        };
    }

    // 1) 子串匹配（最强证据）。
    for (idx, cand) in candidates.iter().enumerate() {
        if normalize_ws(cand).contains(&quote_norm) {
            return CitationVerdict {
                matched: true,
                method: CitationMatch::Substring,
                coverage: 1.0,
                source_index: Some(idx),
            };
        }
    }

    // 2) token 覆盖率匹配。
    let quote_tokens = tokenize(quote);
    if quote_tokens.is_empty() {
        return CitationVerdict {
            matched: false,
            method: CitationMatch::None,
            coverage: 0.0,
            source_index: None,
        };
    }

    let mut best_cov = 0.0f32;
    let mut best_idx: Option<usize> = None;
    for (idx, cand) in candidates.iter().enumerate() {
        let cand_tokens: std::collections::HashSet<String> =
            tokenize(cand).into_iter().collect();
        if cand_tokens.is_empty() {
            continue;
        }
        let hit = quote_tokens
            .iter()
            .filter(|t| cand_tokens.contains(*t))
            .count();
        let cov = hit as f32 / quote_tokens.len() as f32;
        if cov > best_cov {
            best_cov = cov;
            best_idx = Some(idx);
        }
    }

    if best_cov >= TOKEN_OVERLAP_THRESHOLD {
        CitationVerdict {
            matched: true,
            method: CitationMatch::TokenOverlap,
            coverage: best_cov,
            source_index: best_idx,
        }
    } else {
        CitationVerdict {
            matched: false,
            method: CitationMatch::None,
            coverage: best_cov,
            source_index: best_idx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_quote_never_matches() {
        let v = verify_citation("", &["anything"]);
        assert!(!v.matched);
        assert_eq!(v.method, CitationMatch::None);
    }

    #[test]
    fn exact_substring_matches() {
        let v = verify_citation(
            "append-only timeline",
            &["the L0 layer is an append-only timeline of episodes"],
        );
        assert!(v.matched);
        assert_eq!(v.method, CitationMatch::Substring);
        assert_eq!(v.source_index, Some(0));
        assert_eq!(v.coverage, 1.0);
    }

    #[test]
    fn whitespace_and_case_are_normalized() {
        let v = verify_citation(
            "Append-Only   Timeline",
            &["...an append-only\n  timeline..."],
        );
        assert!(v.matched);
        assert_eq!(v.method, CitationMatch::Substring);
    }

    #[test]
    fn picks_correct_candidate_index() {
        let v = verify_citation(
            "stable chunk id",
            &["unrelated text", "we use a stable chunk id as anchor"],
        );
        assert!(v.matched);
        assert_eq!(v.source_index, Some(1));
    }

    #[test]
    fn token_overlap_tolerates_minor_truncation() {
        // 5 个 token，候选缺了结尾标点但词都在 -> 覆盖率 1.0
        let v = verify_citation(
            "memory is augmentation not dependency",
            &["the rule: memory is augmentation, not a dependency in this system"],
        );
        assert!(v.matched);
        // 不是连续子串（because "not a dependency" vs "not dependency"），走 token 覆盖
        assert_eq!(v.method, CitationMatch::TokenOverlap);
        assert!(v.coverage >= 0.8);
    }

    #[test]
    fn fabricated_quote_is_rejected() {
        let v = verify_citation(
            "the system uses quantum entanglement for retrieval",
            &["the system uses a layered keyword and graph retriever"],
        );
        assert!(!v.matched);
        assert_eq!(v.method, CitationMatch::None);
        // covers "the system uses ... for retrieval" partially but below 0.8
        assert!(v.coverage < 0.8);
    }

    #[test]
    fn no_candidates_rejects() {
        let v = verify_citation("anything", &[]);
        assert!(!v.matched);
        assert_eq!(v.source_index, None);
    }
}
