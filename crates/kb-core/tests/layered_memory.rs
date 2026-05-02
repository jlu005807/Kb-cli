use kb_core::domain::{
    KnowledgeChunk, KnowledgeEntry, KnowledgeSnapshot, KnowledgeStatus, SemanticProviderInfo,
};

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
