use std::fs;
use std::path::PathBuf;

use kb_core::domain::{
    GraphSnapshot, IndexSnapshot, KnowledgeSnapshot, SearchProjectionSnapshot,
    WikiManifestSnapshot, WikiViewSnapshot,
};
use kb_core::{FileContentReader, GraphStore, IndexStore, KnowledgeStore, SearchStore, WikiStore};

#[derive(Debug, Clone)]
pub struct FsIndexStore {
    cwd: PathBuf,
}

impl FsIndexStore {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl IndexStore for FsIndexStore {
    fn read_index(&self) -> Result<Option<IndexSnapshot>, String> {
        crate::read_index_files(&self.cwd).map_err(|err| err.to_string())
    }

    fn write_index(&self, snapshot: &IndexSnapshot) -> Result<(), String> {
        crate::write_index_files(&self.cwd, snapshot)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct FsKnowledgeStore {
    cwd: PathBuf,
}

impl FsKnowledgeStore {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl KnowledgeStore for FsKnowledgeStore {
    fn read_knowledge(&self) -> Result<Option<KnowledgeSnapshot>, String> {
        crate::read_knowledge_file(&self.cwd).map_err(|err| err.to_string())
    }

    fn write_knowledge(&self, snapshot: &KnowledgeSnapshot) -> Result<(), String> {
        crate::write_knowledge_file(&self.cwd, snapshot)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct FsSearchStore {
    cwd: PathBuf,
}

impl FsSearchStore {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl SearchStore for FsSearchStore {
    fn read_search(&self) -> Result<Option<SearchProjectionSnapshot>, String> {
        crate::read_search_file(&self.cwd).map_err(|err| err.to_string())
    }

    fn write_search(&self, snapshot: &SearchProjectionSnapshot) -> Result<(), String> {
        crate::write_search_file(&self.cwd, snapshot)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct FsGraphStore {
    cwd: PathBuf,
}

impl FsGraphStore {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl GraphStore for FsGraphStore {
    fn read_graph(&self) -> Result<Option<GraphSnapshot>, String> {
        crate::read_graph_file(&self.cwd).map_err(|err| err.to_string())
    }

    fn write_graph(&self, snapshot: &GraphSnapshot) -> Result<(), String> {
        crate::write_graph_file(&self.cwd, snapshot)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct FsWikiStore {
    cwd: PathBuf,
}

impl FsWikiStore {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl WikiStore for FsWikiStore {
    fn read_wiki_manifest(&self) -> Result<Option<WikiManifestSnapshot>, String> {
        crate::read_wiki_manifest_file(&self.cwd).map_err(|err| err.to_string())
    }

    fn write_wiki_view(
        &self,
        snapshot: &WikiViewSnapshot,
        removed_paths: &[String],
    ) -> Result<(), String> {
        crate::write_wiki_view_files(&self.cwd, snapshot, removed_paths)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct FsFileContentReader {
    cwd: PathBuf,
}

impl FsFileContentReader {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl FileContentReader for FsFileContentReader {
    fn read_text(&self, path: &str) -> Result<String, String> {
        let abs = self.cwd.join(path.trim_start_matches("./"));
        fs::read_to_string(&abs).map_err(|err| format!("failed to read {}: {}", path, err))
    }
}
