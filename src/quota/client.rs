use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use thiserror::Error;

use crate::auth::{load_codex_auth, AuthLoadError};

use super::parser::{parse_quota, QuotaUsage};

pub const QUOTA_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

#[derive(Debug, Clone, Error)]
pub enum QuotaServiceError {
    #[error(transparent)]
    Auth(#[from] AuthLoadError),
    #[error("quota upstream failed: {status} {reason}")]
    Upstream { status: u16, reason: String },
    #[error("quota request failed: {0}")]
    Internal(String),
}

#[async_trait]
pub trait QuotaProvider: Send + Sync {
    async fn fetch(&self) -> Result<QuotaUsage, QuotaServiceError>;
}

#[derive(Clone)]
pub struct QuotaClient {
    client: Client,
    endpoint: Arc<str>,
    auth_path: Option<PathBuf>,
}

impl QuotaClient {
    pub fn new() -> Result<Self, QuotaServiceError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|error| QuotaServiceError::Internal(error.to_string()))?;
        Ok(Self {
            client,
            endpoint: Arc::from(QUOTA_USAGE_URL),
            auth_path: None,
        })
    }

    #[cfg(test)]
    fn with_endpoint_and_auth_path(endpoint: String, client: Client, auth_path: PathBuf) -> Self {
        Self {
            client,
            endpoint: Arc::from(endpoint),
            auth_path: Some(auth_path),
        }
    }

    async fn fetch_with_auth(&self) -> Result<QuotaUsage, QuotaServiceError> {
        let auth = load_codex_auth(self.auth_path.as_deref())?;
        let mut request = self
            .client
            .get(self.endpoint.as_ref())
            .bearer_auth(auth.access_token)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(account_id) = auth.account_id {
            request = request.header("ChatGPT-Account-Id", account_id);
        }

        let response = request
            .send()
            .await
            .map_err(|error| QuotaServiceError::Internal(error.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let reason = status
                .canonical_reason()
                .unwrap_or("upstream returned an error")
                .to_owned();
            return Err(QuotaServiceError::Upstream {
                status: status.as_u16(),
                reason,
            });
        }

        let raw: Value = response
            .json()
            .await
            .map_err(|error| QuotaServiceError::Internal(error.to_string()))?;
        if !raw.is_object() {
            return Err(QuotaServiceError::Internal(
                "quota upstream returned a non-object JSON response".to_owned(),
            ));
        }
        Ok(parse_quota(&raw))
    }
}

#[async_trait]
impl QuotaProvider for QuotaClient {
    async fn fetch(&self) -> Result<QuotaUsage, QuotaServiceError> {
        self.fetch_with_auth().await
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        thread,
        time::Duration,
    };

    use super::{QuotaClient, QuotaProvider, QuotaServiceError};
    use reqwest::Client;

    fn auth_path(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "background_codex_quota_client_{name}_{}_{}.json",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn client_for(endpoint: String, auth_path: PathBuf) -> QuotaClient {
        QuotaClient::with_endpoint_and_auth_path(
            endpoint,
            Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
            auth_path,
        )
    }

    fn spawn_http_server(status: &str, body: &str) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_owned();
        let body = body.to_owned();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut chunk = [0_u8; 2048];
            loop {
                let read = stream.read(&mut chunk).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            String::from_utf8(request).unwrap()
        });
        (format!("http://{address}/usage"), handle)
    }

    #[tokio::test]
    async fn sends_auth_headers_and_parses_object_response() {
        let path = auth_path("success");
        fs::write(
            &path,
            r#"{"tokens":{"access_token":"secret-token","account_id":"account-1"}}"#,
        )
        .unwrap();
        let (endpoint, server) = spawn_http_server(
            "200 OK",
            r#"{"rate_limit":{"primary_window":{"used_percent":28,"limit_window_seconds":604800}}}"#,
        );
        let quota = client_for(endpoint, path.clone()).fetch().await.unwrap();
        let request = server.join().unwrap().to_ascii_lowercase();
        fs::remove_file(path).unwrap();

        assert_eq!(quota.weekly, Some(28));
        assert!(request.contains("authorization: bearer secret-token\r\n"));
        assert!(request.contains("chatgpt-account-id: account-1\r\n"));
        assert!(request.contains("accept: application/json\r\n"));
    }

    #[tokio::test]
    async fn maps_non_success_status_to_upstream_error() {
        let path = auth_path("status");
        fs::write(&path, r#"{"access_token":"secret"}"#).unwrap();
        let (endpoint, server) = spawn_http_server("429 Too Many Requests", r#"{}"#);
        let result = client_for(endpoint, path.clone()).fetch().await;
        server.join().unwrap();
        fs::remove_file(path).unwrap();

        assert!(matches!(
            result,
            Err(QuotaServiceError::Upstream { status: 429, .. })
        ));
    }

    #[tokio::test]
    async fn rejects_non_object_upstream_payload() {
        let path = auth_path("non-object");
        fs::write(&path, r#"{"access_token":"secret"}"#).unwrap();
        let (endpoint, server) = spawn_http_server("200 OK", r#"[]"#);
        let result = client_for(endpoint, path.clone()).fetch().await;
        server.join().unwrap();
        fs::remove_file(path).unwrap();

        assert!(matches!(
            result,
            Err(QuotaServiceError::Internal(message))
                if message.contains("non-object JSON response")
        ));
    }
}
