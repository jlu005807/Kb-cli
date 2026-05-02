use crate::atomic_write::atomic_write_bytes;
use crate::error::{PersistenceError, PersistenceErrorKind};
use kb_core::domain::{WikiManifestSnapshot, WikiViewSnapshot};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WikiViewFiles {
    pub wiki_dir: PathBuf,
    pub manifest_path: PathBuf,
}

impl WikiViewFiles {
    pub fn under(base_dir: &Path) -> Self {
        let wiki_dir = base_dir.join(".kb").join("wiki");
        Self {
            manifest_path: wiki_dir.join("manifest.json"),
            wiki_dir,
        }
    }
}

pub fn read_wiki_manifest_file(
    base_dir: &Path,
) -> Result<Option<WikiManifestSnapshot>, PersistenceError> {
    let files = WikiViewFiles::under(base_dir);
    if !files.manifest_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&files.manifest_path).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.manifest_path.clone(),
            },
            "failed to read wiki manifest",
            err,
        )
    })?;
    let parsed: WikiManifestSnapshot = serde_json::from_str(&raw).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.manifest_path.clone(),
            },
            "failed to parse wiki manifest",
            err,
        )
    })?;
    Ok(Some(parsed))
}

pub fn write_wiki_view_files(
    base_dir: &Path,
    snapshot: &WikiViewSnapshot,
    removed_paths: &[String],
) -> Result<WikiViewFiles, PersistenceError> {
    let files = WikiViewFiles::under(base_dir);
    for rel in removed_paths {
        remove_wiki_relative_file(&files.wiki_dir, rel)?;
    }

    for page in &snapshot.pages {
        let target = files.wiki_dir.join(&page.relative_path);
        atomic_write_bytes(&target, page.markdown.as_bytes())?;
    }
    let manifest_raw = serde_json::to_vec_pretty(&snapshot.manifest).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.manifest_path.clone(),
            },
            "failed to serialize wiki manifest",
            err,
        )
    })?;
    atomic_write_bytes(&files.manifest_path, &manifest_raw)?;
    Ok(files)
}

fn remove_wiki_relative_file(wiki_dir: &Path, rel: &str) -> Result<(), PersistenceError> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| c == Component::ParentDir) {
        return Err(PersistenceError::new(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: wiki_dir.to_path_buf(),
            },
            format!("invalid wiki relative path: {}", rel),
        ));
    }
    let target = wiki_dir.join(rel_path);
    if !target.starts_with(wiki_dir) {
        return Err(PersistenceError::new(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: wiki_dir.to_path_buf(),
            },
            format!("wiki path escapes root: {}", rel),
        ));
    }
    if target.exists() {
        fs::remove_file(&target).map_err(|err| {
            PersistenceError::with_source(
                PersistenceErrorKind::AtomicWriteIo {
                    target_path: target.clone(),
                },
                "failed to remove stale wiki page",
                err,
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{read_wiki_manifest_file, write_wiki_view_files};
    use kb_core::domain::{
        WikiManifestEntry, WikiManifestSnapshot, WikiPageDocument, WikiPageType, WikiViewSnapshot,
    };
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
    fn write_and_read_wiki_manifest_roundtrip() {
        let dir = unique_test_dir("kb-storage-wiki-file");
        fs::create_dir_all(&dir).expect("create temp dir");
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

        write_wiki_view_files(&dir, &snapshot, &[]).expect("write wiki view");
        let manifest = read_wiki_manifest_file(&dir).expect("read wiki manifest");
        assert_eq!(manifest, Some(snapshot.manifest));
        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }

    #[test]
    fn write_wiki_removes_stale_pages() {
        let dir = unique_test_dir("kb-storage-wiki-remove");
        fs::create_dir_all(dir.join(".kb/wiki/pages/source")).expect("create wiki source dir");
        fs::write(dir.join(".kb/wiki/pages/source/old.md"), "# stale page\n")
            .expect("write stale page");

        let snapshot = WikiViewSnapshot {
            manifest: WikiManifestSnapshot {
                schema_version: 1,
                generated_at: "2026-05-02T00:00:00+08:00".to_string(),
                base_graph_signature: "graph:demo".to_string(),
                pages: vec![],
            },
            pages: vec![],
        };

        write_wiki_view_files(&dir, &snapshot, &[String::from("pages/source/old.md")])
            .expect("write wiki view and remove stale");
        assert!(!dir.join(".kb/wiki/pages/source/old.md").exists());
        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }
}
