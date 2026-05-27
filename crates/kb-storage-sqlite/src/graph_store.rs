//! `GraphStore` 的 SQLite 实现。
//!
//! L4 图谱层：节点 + 边 + meta，evidence 是变长 vec，按 JSON blob 存在 edge 行里。
//! schema 中 `(source_id, target_id, edge_type)` 是复合主键；同一对节点之间
//! 不同类型的边可以共存（FileHasTag vs FileRelatedToFile 等）。
//!
//! 写入策略：全量替换。先删 edge（受 FK 约束）再删 node，最后插新数据。

use kb_core::domain::{
    GraphEdge, GraphEdgeType, GraphEvidence, GraphNode, GraphNodeType, GraphSnapshot,
};
use kb_core::error::DomainError;
use kb_core::ports::GraphStore;

use crate::connection::SqliteConnection;

#[derive(Debug, Clone)]
pub struct SqliteGraphStore {
    conn: SqliteConnection,
}

impl SqliteGraphStore {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }
}

impl GraphStore for SqliteGraphStore {
    fn read_graph(&self) -> Result<Option<GraphSnapshot>, DomainError> {
        self.conn.with(|c| {
            let header: Option<(i64, String, String)> = c
                .query_row(
                    "SELECT schema_version, generated_at, base_knowledge_signature \
                     FROM graph_meta WHERE id = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .ok();

            let Some((schema_version, generated_at, base_knowledge_signature)) = header else {
                return Ok(None);
            };

            let mut node_stmt = c
                .prepare(
                    "SELECT id, node_type, label FROM graph_nodes ORDER BY id ASC",
                )
                .map_err(|err| DomainError::io(format!("prepare graph_nodes failed: {err}")))?;
            let nodes = node_stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|err| DomainError::io(format!("query graph_nodes failed: {err}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| DomainError::io(format!("decode graph_node row failed: {err}")))?
                .into_iter()
                .map(|(node_id, type_raw, label)| {
                    Ok(GraphNode {
                        node_id,
                        node_type: decode_node_type(&type_raw)?,
                        label,
                    })
                })
                .collect::<Result<Vec<_>, DomainError>>()?;

            let mut edge_stmt = c
                .prepare(
                    "SELECT source_id, target_id, edge_type, weight, evidence_json \
                     FROM graph_edges ORDER BY source_id ASC, target_id ASC, edge_type ASC",
                )
                .map_err(|err| DomainError::io(format!("prepare graph_edges failed: {err}")))?;
            let edges = edge_stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?, // source_id
                        row.get::<_, String>(1)?, // target_id
                        row.get::<_, String>(2)?, // edge_type
                        row.get::<_, f64>(3)?,    // weight (currently unused on read)
                        row.get::<_, String>(4)?, // evidence_json envelope
                    ))
                })
                .map_err(|err| DomainError::io(format!("query graph_edges failed: {err}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| DomainError::io(format!("decode graph_edge row failed: {err}")))?
                .into_iter()
                .map(|(source_id, target_id, type_raw, _weight, evidence_raw)| {
                    let envelope: GraphEdgeEnvelope = serde_json::from_str(&evidence_raw)
                        .map_err(|err| {
                            DomainError::serialization(format!(
                                "decode graph_edge envelope for '{source_id}->{target_id}/{type_raw}' failed: {err}"
                            ))
                        })?;
                    Ok(GraphEdge {
                        edge_id: envelope.edge_id,
                        edge_type: decode_edge_type(&type_raw)?,
                        from_node_id: source_id,
                        to_node_id: target_id,
                        evidence: envelope.evidence,
                    })
                })
                .collect::<Result<Vec<_>, DomainError>>()?;

            Ok(Some(GraphSnapshot {
                schema_version: schema_version as u32,
                generated_at,
                base_knowledge_signature,
                nodes,
                edges,
            }))
        })
    }

    fn write_graph(&self, snapshot: &GraphSnapshot) -> Result<(), DomainError> {
        self.conn.with_mut(|c| {
            let tx = c
                .transaction()
                .map_err(|err| DomainError::io(format!("begin write_graph tx failed: {err}")))?;

            tx.execute("DELETE FROM graph_edges", [])
                .map_err(|err| DomainError::io(format!("clear graph_edges failed: {err}")))?;
            tx.execute("DELETE FROM graph_nodes", [])
                .map_err(|err| DomainError::io(format!("clear graph_nodes failed: {err}")))?;
            tx.execute("DELETE FROM graph_meta", [])
                .map_err(|err| DomainError::io(format!("clear graph_meta failed: {err}")))?;

            {
                let mut node_stmt = tx
                    .prepare(
                        "INSERT INTO graph_nodes (id, node_type, label) VALUES (?1, ?2, ?3)",
                    )
                    .map_err(|err| {
                        DomainError::io(format!("prepare graph_nodes insert failed: {err}"))
                    })?;
                for node in &snapshot.nodes {
                    node_stmt
                        .execute(rusqlite::params![
                            node.node_id,
                            encode_node_type(&node.node_type),
                            node.label,
                        ])
                        .map_err(|err| {
                            DomainError::conflict(format!(
                                "insert graph_node '{}' failed: {err}",
                                node.node_id
                            ))
                        })?;
                }

                let mut edge_stmt = tx
                    .prepare(
                        "INSERT INTO graph_edges \
                         (source_id, target_id, edge_type, weight, evidence_json) \
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                    )
                    .map_err(|err| {
                        DomainError::io(format!("prepare graph_edges insert failed: {err}"))
                    })?;
                for edge in &snapshot.edges {
                    let envelope = GraphEdgeEnvelope {
                        edge_id: edge.edge_id.clone(),
                        evidence: edge.evidence.clone(),
                    };
                    let envelope_json = serde_json::to_string(&envelope).map_err(|err| {
                        DomainError::serialization(format!(
                            "encode evidence envelope for edge '{}' failed: {err}",
                            edge.edge_id
                        ))
                    })?;
                    edge_stmt
                        .execute(rusqlite::params![
                            edge.from_node_id,
                            edge.to_node_id,
                            encode_edge_type(&edge.edge_type),
                            // weight 当前 domain 不暴露，固定 1.0；保留列以便后续扩展。
                            1.0_f64,
                            envelope_json,
                        ])
                        .map_err(|err| {
                            DomainError::conflict(format!(
                                "insert graph_edge '{}' failed: {err}",
                                edge.edge_id
                            ))
                        })?;
                }
            }

            tx.execute(
                "INSERT INTO graph_meta \
                 (id, schema_version, generated_at, base_knowledge_signature) \
                 VALUES (1, ?1, ?2, ?3)",
                rusqlite::params![
                    snapshot.schema_version as i64,
                    snapshot.generated_at,
                    snapshot.base_knowledge_signature,
                ],
            )
            .map_err(|err| DomainError::io(format!("insert graph_meta failed: {err}")))?;

            tx.commit()
                .map_err(|err| DomainError::io(format!("commit write_graph failed: {err}")))?;
            Ok(())
        })
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct GraphEdgeEnvelope {
    edge_id: String,
    evidence: Vec<GraphEvidence>,
}

fn encode_node_type(n: &GraphNodeType) -> &'static str {
    match n {
        GraphNodeType::File => "file",
        GraphNodeType::Tag => "tag",
        GraphNodeType::Concept => "concept",
        GraphNodeType::Topic => "topic",
    }
}

fn decode_node_type(raw: &str) -> Result<GraphNodeType, DomainError> {
    match raw {
        "file" => Ok(GraphNodeType::File),
        "tag" => Ok(GraphNodeType::Tag),
        "concept" => Ok(GraphNodeType::Concept),
        "topic" => Ok(GraphNodeType::Topic),
        other => Err(DomainError::serialization(format!(
            "unknown graph node type '{other}'"
        ))),
    }
}

fn encode_edge_type(e: &GraphEdgeType) -> &'static str {
    match e {
        GraphEdgeType::FileHasTag => "file_has_tag",
        GraphEdgeType::FileMentionsConcept => "file_mentions_concept",
        GraphEdgeType::ConceptRelatedToConcept => "concept_related_to_concept",
        GraphEdgeType::FileRelatedToFile => "file_related_to_file",
    }
}

fn decode_edge_type(raw: &str) -> Result<GraphEdgeType, DomainError> {
    match raw {
        "file_has_tag" => Ok(GraphEdgeType::FileHasTag),
        "file_mentions_concept" => Ok(GraphEdgeType::FileMentionsConcept),
        "concept_related_to_concept" => Ok(GraphEdgeType::ConceptRelatedToConcept),
        "file_related_to_file" => Ok(GraphEdgeType::FileRelatedToFile),
        other => Err(DomainError::serialization(format!(
            "unknown graph edge type '{other}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ensure_schema;
    use kb_core::domain::{
        GraphEdge, GraphEdgeType, GraphEvidence, GraphNode, GraphNodeType, GraphSnapshot,
    };

    fn fresh_store() -> SqliteGraphStore {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("migrate");
        SqliteGraphStore::new(conn)
    }

    fn sample_snapshot() -> GraphSnapshot {
        GraphSnapshot {
            schema_version: 1,
            generated_at: "2026-05-27T00:00:00+08:00".to_string(),
            base_knowledge_signature: "k-sig".to_string(),
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
                    reason: "tag derived from semantic extract".to_string(),
                }],
            }],
        }
    }

    #[test]
    fn empty_database_returns_none() {
        let store = fresh_store();
        assert_eq!(store.read_graph().expect("read"), None);
    }

    #[test]
    fn write_then_read_roundtrip_preserves_nodes_and_edges() {
        let store = fresh_store();
        let snapshot = sample_snapshot();
        store.write_graph(&snapshot).expect("write");
        let loaded = store.read_graph().expect("read").expect("snapshot");
        assert_eq!(loaded, snapshot);
    }

    #[test]
    fn second_write_replaces_full_snapshot() {
        let store = fresh_store();
        store.write_graph(&sample_snapshot()).expect("first write");

        let replacement = GraphSnapshot {
            schema_version: 1,
            generated_at: "2026-05-27T01:00:00+08:00".to_string(),
            base_knowledge_signature: "k-sig-2".to_string(),
            nodes: vec![],
            edges: vec![],
        };
        store.write_graph(&replacement).expect("second write");
        let loaded = store.read_graph().expect("read").expect("snapshot");
        assert!(loaded.nodes.is_empty());
        assert!(loaded.edges.is_empty());
        assert_eq!(loaded.base_knowledge_signature, "k-sig-2");
    }
}
