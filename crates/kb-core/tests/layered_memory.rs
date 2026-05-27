use kb_core::domain::{
    GraphEdge, GraphEdgeType, GraphEvidence, GraphNode, GraphNodeType, GraphSnapshot, IndexMeta,
    IndexRecord, IndexSnapshot, KnowledgeChunk, KnowledgeEntry, KnowledgeSnapshot, KnowledgeStatus,
    SearchHit, SearchProjectionSnapshot, SemanticProviderInfo, WikiManifestEntry,
    WikiManifestSnapshot, WikiPageDocument, WikiPageType, WikiViewSnapshot,
};
use kb_core::error::DomainError;
use kb_core::ports::{
    FileContentReader, SemanticExtractRequest, SemanticExtractResponse, SemanticExtractor,
};
use kb_core::usecases::{compile_l2, compile_l3, compile_l4, compile_l5};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn knowledge_snapshot_roundtrip_preserves_structure() {
    let snapshot = KnowledgeSnapshot {
        schema_version: 1,
        generated_at: "2026-05-02T00:00:00+08:00".to_string(),
        base_index_signature: "index:demo".to_string(),
        entries: vec![KnowledgeEntry {
            record_id: "docs/a".to_string(),
            path: "./docs/a.md".to_string(),
            input_signature: "f:1".to_string(),
            status: KnowledgeStatus::Active,
            summary: "summary".to_string(),
            tags: vec!["raw".to_string()],
            key_concepts: vec!["concept".to_string()],
            chunks: vec![KnowledgeChunk {
                chunk_id: "docs/a:0001".to_string(),
                byte_range: [0, 64],
                content_signature: "c:1".to_string(),
                summary: "chunk-summary".to_string(),
                preview: "chunk-preview".to_string(),
            }],
            provider: SemanticProviderInfo {
                kind: "rule".to_string(),
                name: "rule-based-v1".to_string(),
            },
        }],
    };

    let raw = serde_json::to_string_pretty(&snapshot).expect("serialize knowledge snapshot");
    let restored: KnowledgeSnapshot =
        serde_json::from_str(&raw).expect("deserialize knowledge snapshot");
    assert_eq!(restored, snapshot);
}

#[test]
fn knowledge_status_uses_snake_case_wire_values() {
    let raw = r#""stale""#;
    let status: KnowledgeStatus = serde_json::from_str(raw).expect("deserialize status");
    assert_eq!(status, KnowledgeStatus::Stale);

    let encoded = serde_json::to_string(&KnowledgeStatus::Missing).expect("serialize status");
    assert_eq!(encoded, r#""missing""#);
}

#[test]
fn search_projection_snapshot_roundtrip_preserves_structure() {
    let snapshot = SearchProjectionSnapshot {
        schema_version: 1,
        generated_at: "2026-05-02T00:00:00+08:00".to_string(),
        base_knowledge_signature: "knowledge:demo".to_string(),
        hits: vec![SearchHit {
            key: "docs/a|docs/a:0001|c:1".to_string(),
            record_id: "docs/a".to_string(),
            chunk_id: "docs/a:0001".to_string(),
            content_signature: "c:1".to_string(),
            score: 0.95,
            matched_terms: vec!["agent".to_string(), "memory".to_string()],
        }],
    };

    let raw = serde_json::to_string_pretty(&snapshot).expect("serialize search snapshot");
    let restored: SearchProjectionSnapshot =
        serde_json::from_str(&raw).expect("deserialize search snapshot");
    assert_eq!(restored, snapshot);
}

#[test]
fn graph_snapshot_roundtrip_preserves_structure() {
    let snapshot = GraphSnapshot {
        schema_version: 1,
        generated_at: "2026-05-02T00:00:00+08:00".to_string(),
        base_knowledge_signature: "knowledge:demo".to_string(),
        nodes: vec![
            GraphNode {
                node_id: "file:docs/a".to_string(),
                node_type: GraphNodeType::File,
                label: "docs/a".to_string(),
            },
            GraphNode {
                node_id: "tag:agent".to_string(),
                node_type: GraphNodeType::Tag,
                label: "agent".to_string(),
            },
        ],
        edges: vec![GraphEdge {
            edge_id: "file_has_tag|file:docs/a|tag:agent".to_string(),
            edge_type: GraphEdgeType::FileHasTag,
            from_node_id: "file:docs/a".to_string(),
            to_node_id: "tag:agent".to_string(),
            evidence: vec![GraphEvidence {
                record_id: "docs/a".to_string(),
                chunk_id: Some("docs/a:0001".to_string()),
                reason: "tag derived from semantic extraction".to_string(),
            }],
        }],
    };

    let raw = serde_json::to_string_pretty(&snapshot).expect("serialize graph snapshot");
    let restored: GraphSnapshot = serde_json::from_str(&raw).expect("deserialize graph snapshot");
    assert_eq!(restored, snapshot);
}

struct FakeReader {
    docs: BTreeMap<String, String>,
    missing: BTreeSet<String>,
}

impl FileContentReader for FakeReader {
    fn read_text(&self, path: &str) -> Result<String, DomainError> {
        if self.missing.contains(path) {
            return Err(DomainError::not_found(format!("source not found: {}", path)));
        }
        self.docs
            .get(path)
            .cloned()
            .ok_or_else(|| DomainError::not_found(format!("unknown test path: {}", path)))
    }
}

struct FakeExtractor {
    fail_records: BTreeSet<String>,
}

impl SemanticExtractor for FakeExtractor {
    fn extract(
        &self,
        request: &SemanticExtractRequest,
    ) -> Result<SemanticExtractResponse, DomainError> {
        if self.fail_records.contains(&request.record_id) {
            return Err(DomainError::external(format!(
                "forced extractor failure: {}",
                request.record_id
            )));
        }

        Ok(SemanticExtractResponse {
            summary: format!("summary({})", request.record_id),
            tags: vec!["tag".to_string()],
            key_concepts: vec!["concept".to_string()],
            chunks: vec![KnowledgeChunk {
                chunk_id: format!("{}:0001", request.record_id),
                byte_range: [0, request.text.len() as u64],
                content_signature: format!("len:{}", request.text.len()),
                summary: "chunk-summary".to_string(),
                preview: request.text.chars().take(32).collect(),
            }],
            provider: SemanticProviderInfo {
                kind: "rule".to_string(),
                name: "rule-based-v1".to_string(),
            },
        })
    }
}

fn index_snapshot(records: Vec<IndexRecord>) -> IndexSnapshot {
    IndexSnapshot {
        records,
        errors: vec![],
        meta: IndexMeta {
            full: false,
            baseline_size: 0,
            tracked_size: 0,
        },
    }
}

#[test]
fn compile_l2_reuses_unchanged_active_entry() {
    let baseline = KnowledgeSnapshot {
        schema_version: 1,
        generated_at: "old".to_string(),
        base_index_signature: "old".to_string(),
        entries: vec![KnowledgeEntry {
            record_id: "docs/a".to_string(),
            path: "./docs/a.md".to_string(),
            input_signature: "./docs/a.md|sig-a".to_string(),
            status: KnowledgeStatus::Active,
            summary: "old-summary".to_string(),
            tags: vec!["old".to_string()],
            key_concepts: vec!["old-concept".to_string()],
            chunks: vec![KnowledgeChunk {
                chunk_id: "docs/a:0001".to_string(),
                byte_range: [0, 10],
                content_signature: "old".to_string(),
                summary: "old".to_string(),
                preview: "old".to_string(),
            }],
            provider: SemanticProviderInfo {
                kind: "rule".to_string(),
                name: "rule-based-v1".to_string(),
            },
        }],
    };
    let snapshot = index_snapshot(vec![IndexRecord {
        alias: "docs/a".to_string(),
        path: "./docs/a.md".to_string(),
        category: "raw".to_string(),
        signature: "sig-a".to_string(),
        present: true,
        excerpt: None,
    }]);

    let reader = FakeReader {
        docs: BTreeMap::new(),
        missing: BTreeSet::new(),
    };
    let extractor = FakeExtractor {
        fail_records: BTreeSet::new(),
    };

    let out = compile_l2(Some(&baseline), &snapshot, &reader, &extractor, "now");
    assert_eq!(out.reused, 1);
    assert_eq!(out.processed, 0);
    assert_eq!(out.missing, 0);
    assert_eq!(out.failed, 0);
    assert_eq!(out.issues.len(), 0);
    assert_eq!(out.snapshot.entries[0].summary, "old-summary");
}

#[test]
fn compile_l2_marks_missing_and_failed_entries() {
    let snapshot = index_snapshot(vec![
        IndexRecord {
            alias: "docs/missing".to_string(),
            path: "./docs/missing.md".to_string(),
            category: "raw".to_string(),
            signature: "sig-m".to_string(),
            present: true,
            excerpt: None,
        },
        IndexRecord {
            alias: "docs/fail".to_string(),
            path: "./docs/fail.md".to_string(),
            category: "raw".to_string(),
            signature: "sig-f".to_string(),
            present: true,
            excerpt: None,
        },
    ]);

    let reader = FakeReader {
        docs: BTreeMap::from([(
            "./docs/fail.md".to_string(),
            "line one\nline two".to_string(),
        )]),
        missing: BTreeSet::from(["./docs/missing.md".to_string()]),
    };
    let extractor = FakeExtractor {
        fail_records: BTreeSet::from(["docs/fail".to_string()]),
    };

    let out = compile_l2(None, &snapshot, &reader, &extractor, "now");
    assert_eq!(out.reused, 0);
    assert_eq!(out.processed, 2);
    assert_eq!(out.missing, 1);
    assert_eq!(out.failed, 1);
    assert_eq!(out.issues.len(), 2);

    let missing_entry = out
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.record_id == "docs/missing")
        .expect("missing entry");
    assert_eq!(missing_entry.status, KnowledgeStatus::Missing);

    let failed_entry = out
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.record_id == "docs/fail")
        .expect("failed entry");
    assert_eq!(failed_entry.status, KnowledgeStatus::Failed);
    assert_eq!(failed_entry.summary, "line one");
}

#[test]
fn compile_l3_reuses_unchanged_hits() {
    let knowledge = KnowledgeSnapshot {
        schema_version: 1,
        generated_at: "now".to_string(),
        base_index_signature: "idx".to_string(),
        entries: vec![KnowledgeEntry {
            record_id: "docs/a".to_string(),
            path: "./docs/a.md".to_string(),
            input_signature: "./docs/a.md|sig-a".to_string(),
            status: KnowledgeStatus::Active,
            summary: "summary".to_string(),
            tags: vec!["agent".to_string()],
            key_concepts: vec!["Memory Model".to_string()],
            chunks: vec![KnowledgeChunk {
                chunk_id: "docs/a:0001".to_string(),
                byte_range: [0, 12],
                content_signature: "c:1".to_string(),
                summary: "chunk summary".to_string(),
                preview: "chunk preview".to_string(),
            }],
            provider: SemanticProviderInfo {
                kind: "rule".to_string(),
                name: "rule-based-v1".to_string(),
            },
        }],
    };

    let baseline = SearchProjectionSnapshot {
        schema_version: 1,
        generated_at: "old".to_string(),
        base_knowledge_signature: "old".to_string(),
        hits: vec![SearchHit {
            key: "docs/a|docs/a:0001|c:1".to_string(),
            record_id: "docs/a".to_string(),
            chunk_id: "docs/a:0001".to_string(),
            content_signature: "c:1".to_string(),
            score: 0.91,
            matched_terms: vec!["agent".to_string(), "memory".to_string()],
        }],
    };

    let out = compile_l3(Some(&baseline), &knowledge, "new");
    assert_eq!(out.reused, 1);
    assert_eq!(out.processed, 0);
    assert_eq!(out.snapshot.hits.len(), 1);
    assert_eq!(out.snapshot.hits[0].score, 0.91);
}

#[test]
fn compile_l3_rebuilds_changed_chunks_and_prunes_non_active_entries() {
    let knowledge = KnowledgeSnapshot {
        schema_version: 1,
        generated_at: "now".to_string(),
        base_index_signature: "idx".to_string(),
        entries: vec![
            KnowledgeEntry {
                record_id: "docs/a".to_string(),
                path: "./docs/a.md".to_string(),
                input_signature: "./docs/a.md|sig-a2".to_string(),
                status: KnowledgeStatus::Active,
                summary: "summary".to_string(),
                tags: vec!["agent".to_string()],
                key_concepts: vec!["Memory".to_string()],
                chunks: vec![KnowledgeChunk {
                    chunk_id: "docs/a:0001".to_string(),
                    byte_range: [0, 10],
                    content_signature: "c:2".to_string(),
                    summary: "new chunk".to_string(),
                    preview: "new chunk preview".to_string(),
                }],
                provider: SemanticProviderInfo {
                    kind: "rule".to_string(),
                    name: "rule-based-v1".to_string(),
                },
            },
            KnowledgeEntry {
                record_id: "docs/b".to_string(),
                path: "./docs/b.md".to_string(),
                input_signature: "./docs/b.md|sig-b".to_string(),
                status: KnowledgeStatus::Missing,
                summary: String::new(),
                tags: vec![],
                key_concepts: vec![],
                chunks: vec![],
                provider: SemanticProviderInfo {
                    kind: "system".to_string(),
                    name: "missing-source".to_string(),
                },
            },
        ],
    };

    let baseline = SearchProjectionSnapshot {
        schema_version: 1,
        generated_at: "old".to_string(),
        base_knowledge_signature: "old".to_string(),
        hits: vec![
            SearchHit {
                key: "docs/a|docs/a:0001|c:1".to_string(),
                record_id: "docs/a".to_string(),
                chunk_id: "docs/a:0001".to_string(),
                content_signature: "c:1".to_string(),
                score: 0.77,
                matched_terms: vec!["legacy".to_string()],
            },
            SearchHit {
                key: "docs/b|docs/b:0001|c:x".to_string(),
                record_id: "docs/b".to_string(),
                chunk_id: "docs/b:0001".to_string(),
                content_signature: "c:x".to_string(),
                score: 0.42,
                matched_terms: vec!["legacy".to_string()],
            },
        ],
    };

    let out = compile_l3(Some(&baseline), &knowledge, "new");
    assert_eq!(out.reused, 0);
    assert_eq!(out.processed, 1);
    assert_eq!(out.snapshot.hits.len(), 1);
    assert_eq!(out.snapshot.hits[0].key, "docs/a|docs/a:0001|c:2");
}

#[test]
fn graph_builder_reuses_unchanged_edges() {
    let knowledge = KnowledgeSnapshot {
        schema_version: 1,
        generated_at: "now".to_string(),
        base_index_signature: "idx".to_string(),
        entries: vec![
            KnowledgeEntry {
                record_id: "docs/a".to_string(),
                path: "./docs/a.md".to_string(),
                input_signature: "./docs/a.md|sig-a".to_string(),
                status: KnowledgeStatus::Active,
                summary: "summary".to_string(),
                tags: vec!["agent".to_string()],
                key_concepts: vec!["memory model".to_string()],
                chunks: vec![KnowledgeChunk {
                    chunk_id: "docs/a:0001".to_string(),
                    byte_range: [0, 12],
                    content_signature: "c:1".to_string(),
                    summary: "chunk summary".to_string(),
                    preview: "chunk preview".to_string(),
                }],
                provider: SemanticProviderInfo {
                    kind: "rule".to_string(),
                    name: "rule-based-v1".to_string(),
                },
            },
            KnowledgeEntry {
                record_id: "docs/b".to_string(),
                path: "./docs/b.md".to_string(),
                input_signature: "./docs/b.md|sig-b".to_string(),
                status: KnowledgeStatus::Active,
                summary: "summary".to_string(),
                tags: vec!["agent".to_string(), "rag".to_string()],
                key_concepts: vec!["memory model".to_string()],
                chunks: vec![KnowledgeChunk {
                    chunk_id: "docs/b:0001".to_string(),
                    byte_range: [0, 10],
                    content_signature: "c:2".to_string(),
                    summary: "chunk summary".to_string(),
                    preview: "chunk preview".to_string(),
                }],
                provider: SemanticProviderInfo {
                    kind: "rule".to_string(),
                    name: "rule-based-v1".to_string(),
                },
            },
        ],
    };

    let first = compile_l4(None, &knowledge, "gen-1");
    assert_eq!(first.invalid_edges, 0);
    assert_eq!(first.issues.len(), 0);
    assert!(!first.snapshot.edges.is_empty());

    let second = compile_l4(Some(&first.snapshot), &knowledge, "gen-2");
    assert_eq!(second.processed, 0);
    assert_eq!(second.reused, first.snapshot.edges.len());
}

#[test]
fn graph_builder_enforces_evidence_and_prunes_non_active_entries() {
    let knowledge = KnowledgeSnapshot {
        schema_version: 1,
        generated_at: "now".to_string(),
        base_index_signature: "idx".to_string(),
        entries: vec![
            KnowledgeEntry {
                record_id: "docs/a".to_string(),
                path: "./docs/a.md".to_string(),
                input_signature: "./docs/a.md|sig-a".to_string(),
                status: KnowledgeStatus::Active,
                summary: "summary".to_string(),
                tags: vec!["agent".to_string()],
                key_concepts: vec!["memory".to_string(), "model".to_string()],
                chunks: vec![KnowledgeChunk {
                    chunk_id: "docs/a:0001".to_string(),
                    byte_range: [0, 12],
                    content_signature: "c:1".to_string(),
                    summary: "chunk summary".to_string(),
                    preview: "chunk preview".to_string(),
                }],
                provider: SemanticProviderInfo {
                    kind: "rule".to_string(),
                    name: "rule-based-v1".to_string(),
                },
            },
            KnowledgeEntry {
                record_id: "docs/missing".to_string(),
                path: "./docs/missing.md".to_string(),
                input_signature: "./docs/missing.md|sig-m".to_string(),
                status: KnowledgeStatus::Missing,
                summary: String::new(),
                tags: vec!["agent".to_string()],
                key_concepts: vec!["memory".to_string()],
                chunks: vec![],
                provider: SemanticProviderInfo {
                    kind: "system".to_string(),
                    name: "missing-source".to_string(),
                },
            },
        ],
    };

    let out = compile_l4(None, &knowledge, "gen-1");
    assert_eq!(out.invalid_edges, 0);
    assert_eq!(out.issues.len(), 0);
    assert!(out.processed > 0);
    assert!(out
        .snapshot
        .nodes
        .iter()
        .all(|node| !node.node_id.contains("docs/missing")));
    assert!(out
        .snapshot
        .edges
        .iter()
        .all(|edge| !edge.evidence.is_empty()));
}

#[test]
fn wiki_view_snapshot_roundtrip_preserves_structure() {
    let snapshot = WikiViewSnapshot {
        manifest: WikiManifestSnapshot {
            schema_version: 1,
            generated_at: "2026-05-02T00:00:00+08:00".to_string(),
            base_graph_signature: "graph:demo".to_string(),
            pages: vec![WikiManifestEntry {
                page_id: "index".to_string(),
                page_type: WikiPageType::Index,
                title: "Knowledge Wiki".to_string(),
                relative_path: "index.md".to_string(),
                input_signature: "sig:index".to_string(),
                source_records: vec![],
                source_chunks: vec![],
            }],
        },
        pages: vec![WikiPageDocument {
            page_id: "index".to_string(),
            page_type: WikiPageType::Index,
            title: "Knowledge Wiki".to_string(),
            relative_path: "index.md".to_string(),
            input_signature: "sig:index".to_string(),
            source_records: vec![],
            source_chunks: vec![],
            markdown: "# Knowledge Wiki\n".to_string(),
        }],
    };

    let raw = serde_json::to_string_pretty(&snapshot).expect("serialize wiki view snapshot");
    let restored: WikiViewSnapshot =
        serde_json::from_str(&raw).expect("deserialize wiki view snapshot");
    assert_eq!(restored, snapshot);
}

#[test]
fn wiki_view_builder_reuses_unchanged_pages() {
    let knowledge = KnowledgeSnapshot {
        schema_version: 1,
        generated_at: "now".to_string(),
        base_index_signature: "idx".to_string(),
        entries: vec![KnowledgeEntry {
            record_id: "docs/a".to_string(),
            path: "./docs/a.md".to_string(),
            input_signature: "./docs/a.md|sig-a".to_string(),
            status: KnowledgeStatus::Active,
            summary: "summary".to_string(),
            tags: vec!["agent".to_string()],
            key_concepts: vec!["memory model".to_string()],
            chunks: vec![KnowledgeChunk {
                chunk_id: "docs/a:0001".to_string(),
                byte_range: [0, 12],
                content_signature: "c:1".to_string(),
                summary: "chunk summary".to_string(),
                preview: "chunk preview".to_string(),
            }],
            provider: SemanticProviderInfo {
                kind: "rule".to_string(),
                name: "rule-based-v1".to_string(),
            },
        }],
    };
    let graph = compile_l4(None, &knowledge, "g1").snapshot;

    let first = compile_l5(None, &knowledge, &graph, "w1");
    assert!(first.snapshot.manifest.pages.len() >= 4);
    assert_eq!(first.issues.len(), 0);

    let second = compile_l5(Some(&first.snapshot.manifest), &knowledge, &graph, "w2");
    assert!(second.reused >= 1);
    assert_eq!(
        second.processed + second.reused,
        first.snapshot.manifest.pages.len()
    );
    assert!(second.removed_paths.is_empty());
}

#[test]
fn wiki_view_builder_tracks_removed_pages() {
    let knowledge = KnowledgeSnapshot {
        schema_version: 1,
        generated_at: "now".to_string(),
        base_index_signature: "idx".to_string(),
        entries: vec![KnowledgeEntry {
            record_id: "docs/a".to_string(),
            path: "./docs/a.md".to_string(),
            input_signature: "./docs/a.md|sig-a".to_string(),
            status: KnowledgeStatus::Active,
            summary: "summary".to_string(),
            tags: vec!["agent".to_string()],
            key_concepts: vec!["memory model".to_string()],
            chunks: vec![KnowledgeChunk {
                chunk_id: "docs/a:0001".to_string(),
                byte_range: [0, 12],
                content_signature: "c:1".to_string(),
                summary: "chunk summary".to_string(),
                preview: "chunk preview".to_string(),
            }],
            provider: SemanticProviderInfo {
                kind: "rule".to_string(),
                name: "rule-based-v1".to_string(),
            },
        }],
    };
    let graph = compile_l4(None, &knowledge, "g1").snapshot;
    let baseline = compile_l5(None, &knowledge, &graph, "w1");

    let empty_knowledge = KnowledgeSnapshot {
        schema_version: 1,
        generated_at: "now".to_string(),
        base_index_signature: "idx".to_string(),
        entries: vec![],
    };
    let empty_graph = compile_l4(None, &empty_knowledge, "g2").snapshot;
    let next = compile_l5(
        Some(&baseline.snapshot.manifest),
        &empty_knowledge,
        &empty_graph,
        "w2",
    );

    assert!(next
        .removed_paths
        .iter()
        .any(|path| path.starts_with("pages/source/")));
}
