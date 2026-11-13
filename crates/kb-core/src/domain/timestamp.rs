//! `Timestamp` 是 unix nanoseconds 的 newtype，作为 L0/L2 记忆层的时间锚。
//!
//! 选用 u64 + nanos 是为了：
//! - 单调且可全序比较（agent 多并发写入也能稳定排序）
//! - 不引入 `chrono` 依赖，保持 kb-core 零外部 crate
//! - 序列化为单个数字，JSON / SQLite 通用
//!
//! 与 wall-clock 字符串（`KnowledgeSnapshot.generated_at`）分工：
//! 字符串便于人读 / diff，`Timestamp` 便于代码层算 decay 与 last_verified。

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub const ZERO: Timestamp = Timestamp(0);

    /// 当前 unix 纳秒。如果系统时钟早于 UNIX_EPOCH（不应该发生）回退到 0
    /// 以保证函数 total——记忆层任何写入路径都不希望因为时钟异常 panic。
    pub fn now() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Self(nanos)
    }

    pub fn as_nanos(self) -> u64 {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Self::ZERO
    }
}

impl From<u64> for Timestamp {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_is_total() {
        let a = Timestamp(10);
        let b = Timestamp(20);
        assert!(a < b);
        assert!(b > a);
    }

    #[test]
    fn serde_roundtrip_is_transparent() {
        let t = Timestamp(1_234_567_890);
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "1234567890");
        let back: Timestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn zero_is_default() {
        assert_eq!(Timestamp::default(), Timestamp::ZERO);
        assert!(Timestamp::ZERO.is_zero());
    }
}
