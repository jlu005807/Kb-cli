use kb_core::domain::IndexSnapshot;

// 轻量内存后端：用于测试或无持久化场景。
#[derive(Debug, Default, Clone)]
pub struct InMemoryIndexStore {
    snapshot: Option<IndexSnapshot>,
}

impl InMemoryIndexStore {
    /// 创建空内存存储。
    pub fn new() -> Self {
        Self { snapshot: None }
    }

    /// 读取当前快照（返回克隆值，保持调用方与内部状态隔离）。
    pub fn load(&self) -> Option<IndexSnapshot> {
        self.snapshot.clone()
    }

    /// 覆盖保存快照。
    pub fn save(&mut self, snapshot: IndexSnapshot) {
        self.snapshot = Some(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::InMemoryIndexStore;
    use kb_core::domain::{IndexMeta, IndexSnapshot};

    #[test]
    fn stores_and_loads_snapshot() {
        let mut store = InMemoryIndexStore::new();
        let snapshot = IndexSnapshot {
            records: vec![],
            errors: vec![],
            meta: IndexMeta {
                full: false,
                baseline_size: 0,
                tracked_size: 0,
            },
        };
        store.save(snapshot.clone());
        assert_eq!(store.load(), Some(snapshot));
    }
}
