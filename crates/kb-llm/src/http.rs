//! 异步 HTTP 客户端 + 共享 Tokio runtime。
//!
//! 设计目标：
//! - `kb-llm` 暴露的 `SemanticExtractor` 等 port 仍是同步签名（避免把 async
//!   传染到 `kb-core` 和 `kb-cli`），但内部用 `reqwest` 走 HTTP/2 + 连接复用，
//!   显著优于 `ureq` 的阻塞短连接。
//! - 用一个进程内共享的多线程 runtime 驱动异步任务，避免每次请求都 `Runtime::new()`
//!   导致的高额初始化开销，也方便后续 #9 part 2 在同一个 runtime 内并行编译。
//! - 通过 `block_on_with_timeout` 把异步 future 桥回同步调用方，并强制施加超时，
//!   防止 LLM 卡住调用线程。
//!
//! 该模块**不**关心 OpenAI Chat Completions 的协议字段，仅负责传输层。

use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::OnceCell;
use reqwest::{header, Client};
use serde_json::Value;
use tokio::runtime::{Builder, Runtime};

use kb_core::error::DomainError;

/// 进程内共享 Tokio runtime。
///
/// 第一次访问时按 CPU 核心数构建 multi-thread runtime；后续所有 LLM 调用都复用
/// 这一个 runtime。runtime 的生命周期跟随进程，process exit 时由 OS 回收。
fn shared_runtime() -> &'static Runtime {
    static RUNTIME: OnceCell<Runtime> = OnceCell::new();
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .thread_name("kb-llm-rt")
            .build()
            .expect("failed to start kb-llm tokio runtime")
    })
}

/// 共享 `reqwest::Client`：连接池 + HTTP/2 自动协商。
///
/// 每个客户端独立持有连接池，因此跨 LLM provider 的请求复用 TCP/TLS 连接。
fn shared_client() -> &'static Client {
    static CLIENT: OnceCell<Client> = OnceCell::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent(concat!("kb-cli/", env!("CARGO_PKG_VERSION")))
            .pool_max_idle_per_host(8)
            .build()
            .expect("failed to build kb-llm reqwest client")
    })
}

/// 异步 HTTP 客户端：POST JSON、读取 JSON 响应，封装 OpenAI 兼容协议常见错误。
#[derive(Debug, Clone)]
pub struct HttpClient {
    timeout: Duration,
}

impl HttpClient {
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs.max(1)),
        }
    }

    /// 向 OpenAI 兼容 endpoint 发 JSON POST，并以 JSON 解析响应体。
    ///
    /// - 4xx/5xx 在网络层成功但业务失败，会返回 `DomainError::external` 携带状态码与正文。
    /// - 超时、TCP 重置、DNS 失败 → `DomainError::io`（属于瞬时类，可重试）。
    /// - 非法 JSON 响应 → `DomainError::serialization`。
    pub async fn post_json(
        &self,
        url: &str,
        api_key: &str,
        body: &Value,
    ) -> Result<Value, DomainError> {
        let response = shared_client()
            .post(url)
            .timeout(self.timeout)
            .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
            .header(header::CONTENT_TYPE, "application/json")
            .json(body)
            .send()
            .await
            .map_err(|err| DomainError::io(format!("llm http transport failed: {err}")))?;

        let status = response.status();
        let raw = response
            .text()
            .await
            .map_err(|err| DomainError::io(format!("llm http read body failed: {err}")))?;

        if !status.is_success() {
            return Err(DomainError::external(format!(
                "llm http {} response: {}",
                status.as_u16(),
                truncate_for_error(&raw, 800)
            )));
        }

        serde_json::from_str(&raw).map_err(|err| {
            DomainError::serialization(format!(
                "llm http response was not valid json ({}): {}",
                err,
                truncate_for_error(&raw, 400)
            ))
        })
    }
}

/// 在共享 runtime 上阻塞地执行一个 future。
///
/// 调用方通常是 sync port 实现（例如 `SemanticExtractor::extract`），用这个桥
/// 完成"对外同步、对内异步"的折中策略。
///
/// 注意：不要在异步上下文中调用——会触发 tokio 的 `block_in_place` 警告或死锁。
pub fn block_on<F>(fut: F) -> F::Output
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    // 使用 spawn + blocking handle，使得这个调用本身可以来自任意线程
    // （包括 rayon worker、std thread 等），无需调用方手动管理 runtime guard。
    let handle = shared_runtime().spawn(fut);
    shared_runtime().block_on(handle).expect("kb-llm task panicked")
}

/// 与 `block_on` 类似，但额外施加一个上限超时，防止上游 LLM 阻塞过久。
///
/// 超过 `timeout` 会返回 `DomainError::external("timeout after ...")`，由调用方
/// 决定是否触发重试。
pub fn block_on_with_timeout<F, T>(timeout: Duration, fut: F) -> Result<T, DomainError>
where
    F: std::future::Future<Output = Result<T, DomainError>> + Send + 'static,
    T: Send + 'static,
{
    block_on(async move {
        match tokio::time::timeout(timeout, fut).await {
            Ok(inner) => inner,
            Err(_) => Err(DomainError::external(format!(
                "llm operation timed out after {}s",
                timeout.as_secs().max(1)
            ))),
        }
    })
}

/// 把 provider 返回的长文本截断成"足够定位、又不会撑爆错误日志"的长度。
fn truncate_for_error(raw: &str, max_chars: usize) -> String {
    if raw.chars().count() <= max_chars {
        return raw.to_string();
    }
    let mut out: String = raw.chars().take(max_chars).collect();
    out.push_str("...<truncated>");
    out
}

/// 用于把任意 `Send` 闭包提交到共享 runtime 上的便捷类型别名。
pub type SharedRuntime = Arc<()>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_on_completes_simple_future() {
        let value = block_on(async { 21 + 21 });
        assert_eq!(value, 42);
    }

    #[test]
    fn block_on_with_timeout_returns_inner_ok() {
        let value: u32 = block_on_with_timeout(Duration::from_secs(1), async { Ok(7u32) })
            .expect("inner future should return Ok");
        assert_eq!(value, 7);
    }

    #[test]
    fn block_on_with_timeout_reports_timeout() {
        let err = block_on_with_timeout::<_, ()>(Duration::from_millis(50), async {
            tokio::time::sleep(Duration::from_secs(2)).await;
            Ok(())
        })
        .expect_err("should time out");
        assert!(err.to_string().contains("timed out"), "got: {err}");
    }

    #[test]
    fn truncate_for_error_respects_char_boundary() {
        let s = "héllo world".to_string();
        assert_eq!(truncate_for_error(&s, 100), s);
        let cut = truncate_for_error(&s, 4);
        assert!(cut.starts_with("héll"));
        assert!(cut.ends_with("<truncated>"));
    }
}
