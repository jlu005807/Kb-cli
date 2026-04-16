use crate::error::{PersistenceError, PersistenceErrorKind};
use crate::schema::{read_schema_version, write_schema_version, SchemaStatus};
use std::path::Path;

pub const SCHEMA_VERSION_CURRENT: &str = "1.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationStep {
    InitializeToCurrent { to: String },
    UpgradeToCurrent { from: String, to: String },
    NoopCurrent { version: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPlan {
    pub from: Option<String>,
    pub to: String,
    pub steps: Vec<MigrationStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationOutcome {
    pub from: Option<String>,
    pub to: String,
    pub migrated: bool,
}

// ensure_schema：执行“必要的最小迁移”。
// - 缺失：初始化为当前版本
// - 旧版本：升级到当前版本
// - 新版本：拒绝（防止旧二进制写坏新数据）
pub fn ensure_schema(kb_dir: &Path) -> Result<MigrationOutcome, PersistenceError> {
    let current = SCHEMA_VERSION_CURRENT.to_string();
    match read_schema_version(kb_dir)? {
        SchemaStatus::Missing => {
            write_schema_version(kb_dir, &current)?;
            Ok(MigrationOutcome {
                from: None,
                to: current,
                migrated: true,
            })
        }
        SchemaStatus::Version(found) if found == current => Ok(MigrationOutcome {
            from: Some(found),
            to: current,
            migrated: false,
        }),
        SchemaStatus::Version(found) => {
            if found.as_str() < SCHEMA_VERSION_CURRENT {
                write_schema_version(kb_dir, &current)?;
                Ok(MigrationOutcome {
                    from: Some(found),
                    to: current,
                    migrated: true,
                })
            } else {
                Err(PersistenceError::new(
                    PersistenceErrorKind::SchemaUnsupported {
                        found,
                        current,
                        schema_path: kb_dir.join("schema.version"),
                    },
                    "schema version is newer than supported binary",
                ))
            }
        }
    }
}

pub fn plan_migration(kb_dir: &Path) -> Result<MigrationPlan, PersistenceError> {
    // 仅规划，不落盘。
    let to = SCHEMA_VERSION_CURRENT.to_string();
    let status = read_schema_version(kb_dir)?;

    match status {
        SchemaStatus::Missing => Ok(MigrationPlan {
            from: None,
            to: to.clone(),
            steps: vec![MigrationStep::InitializeToCurrent { to }],
        }),
        SchemaStatus::Version(found) if found == to => Ok(MigrationPlan {
            from: Some(found.clone()),
            to,
            steps: vec![MigrationStep::NoopCurrent { version: found }],
        }),
        SchemaStatus::Version(found) if found.as_str() < SCHEMA_VERSION_CURRENT => {
            Ok(MigrationPlan {
                from: Some(found.clone()),
                to: to.clone(),
                steps: vec![MigrationStep::UpgradeToCurrent { from: found, to }],
            })
        }
        SchemaStatus::Version(found) => Err(PersistenceError::new(
            PersistenceErrorKind::SchemaUnsupported {
                found,
                current: to,
                schema_path: kb_dir.join("schema.version"),
            },
            "schema version is newer than supported binary",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_schema, plan_migration, MigrationStep, SCHEMA_VERSION_CURRENT};
    use crate::error::PersistenceErrorKind;
    use crate::schema::write_schema_version;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("{}-{}", prefix, nonce))
    }

    #[test]
    fn ensure_schema_initializes_when_missing() {
        let dir = unique_test_dir("kb-storage-fs-schema-init");
        fs::create_dir_all(&dir).expect("create temp dir");

        let outcome = ensure_schema(&dir).expect("ensure schema should initialize");
        assert_eq!(outcome.from, None);
        assert_eq!(outcome.to, SCHEMA_VERSION_CURRENT);
        assert!(outcome.migrated);

        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }

    #[test]
    fn plan_and_ensure_schema_upgrade_older_version() {
        let dir = unique_test_dir("kb-storage-fs-schema-upgrade");
        fs::create_dir_all(&dir).expect("create temp dir");
        write_schema_version(&dir, "0.9").expect("write older schema");

        let plan = plan_migration(&dir).expect("plan migration should succeed");
        assert_eq!(plan.from, Some("0.9".to_string()));
        assert_eq!(plan.to, SCHEMA_VERSION_CURRENT);
        assert_eq!(
            plan.steps,
            vec![MigrationStep::UpgradeToCurrent {
                from: "0.9".to_string(),
                to: SCHEMA_VERSION_CURRENT.to_string()
            }]
        );

        let outcome = ensure_schema(&dir).expect("ensure schema upgrade should succeed");
        assert_eq!(outcome.from, Some("0.9".to_string()));
        assert!(outcome.migrated);

        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }

    #[test]
    fn ensure_schema_rejects_newer_version_deterministically() {
        let dir = unique_test_dir("kb-storage-fs-schema-newer");
        fs::create_dir_all(&dir).expect("create temp dir");
        write_schema_version(&dir, "9.0").expect("write newer schema");

        let err = ensure_schema(&dir).expect_err("newer schema must fail");
        match err.kind {
            PersistenceErrorKind::SchemaUnsupported { found, current, .. } => {
                assert_eq!(found, "9.0");
                assert_eq!(current, SCHEMA_VERSION_CURRENT);
            }
            other => panic!("unexpected error kind: {other:?}"),
        }

        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }
}
