use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

use crate::{Error, Result, USER_AGENT};

/// 复用一个连接池：清单、补丁和整包下载共享 keep-alive 连接。
pub(crate) struct HttpClient {
    agent: ureq::Agent,
}

impl HttpClient {
    /// 启动器文件下载共用同一个连接池，避免每个小文件重新握手。
    pub(crate) fn shared() -> &'static HttpClient {
        static CLIENT: OnceLock<HttpClient> = OnceLock::new();
        CLIENT.get_or_init(HttpClient::new)
    }

    pub(crate) fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .max_idle_connections(16)
            .max_idle_connections_per_host(8)
            .timeout_global(Some(Duration::from_secs(120)))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }

    pub(crate) fn request(
        &self,
        url: &str,
        timeout: Duration,
    ) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
        self.agent
            .get(url)
            .header("User-Agent", USER_AGENT)
            .config()
            .timeout_global(Some(timeout))
            .build()
    }

    pub(crate) fn fetch_bytes(&self, url: &str, limit: u64, timeout: Duration) -> Result<Vec<u8>> {
        let mut response = self
            .request(url, timeout)
            .call()
            .map_err(|error| Error::Http(format!("GET {url}: {error}")))?;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(limit)
            .read_to_end(&mut bytes)
            .map_err(|error| Error::Http(format!("read {url}: {error}")))?;
        Ok(bytes)
    }
}
