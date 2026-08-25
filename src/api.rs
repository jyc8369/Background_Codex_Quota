use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use crate::{
    control::ShutdownSignal,
    quota::{
        client::{QuotaProvider, QuotaServiceError},
        parser::QuotaUsage,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub provider: Arc<dyn QuotaProvider>,
    pub shutdown: Arc<ShutdownSignal>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct ControlResponse {
    status: &'static str,
}

#[derive(Debug)]
enum ApiError {
    Auth(String),
    Upstream { status: u16, reason: String },
    Internal,
}

impl From<QuotaServiceError> for ApiError {
    fn from(error: QuotaServiceError) -> Self {
        match error {
            QuotaServiceError::Auth(error) => Self::Auth(error.to_string()),
            QuotaServiceError::Upstream { status, reason } => Self::Upstream { status, reason },
            QuotaServiceError::Internal(error) => {
                eprintln!("quota request failed: {error}");
                Self::Internal
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            Self::Auth(detail) => (
                StatusCode::UNAUTHORIZED,
                serde_json::json!({"detail": detail}),
            ),
            Self::Upstream { status, reason } => (
                StatusCode::BAD_GATEWAY,
                serde_json::json!({
                    "detail": {
                        "message": "quota upstream request failed",
                        "upstreamStatus": status,
                        "reason": reason,
                    }
                }),
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"detail": "quota request failed"}),
            ),
        };
        (status, Json(body)).into_response()
    }
}

pub fn router(provider: Arc<dyn QuotaProvider>, shutdown: Arc<ShutdownSignal>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/quota/usage", get(quota_usage))
        .route("/control/shutdown", post(shutdown_backend))
        .with_state(AppState { provider, shutdown })
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn quota_usage(State(state): State<AppState>) -> Result<Json<QuotaUsage>, ApiError> {
    Ok(Json(state.provider.fetch().await.map_err(ApiError::from)?))
}

async fn shutdown_backend(State(state): State<AppState>) -> Response {
    state.shutdown.request();
    (
        StatusCode::ACCEPTED,
        Json(ControlResponse {
            status: "shutting_down",
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::router;
    use crate::{
        control::ShutdownSignal,
        quota::{
            client::{QuotaProvider, QuotaServiceError},
            parser::QuotaUsage,
        },
    };

    struct StubProvider {
        result: Result<QuotaUsage, QuotaServiceError>,
    }

    #[async_trait]
    impl QuotaProvider for StubProvider {
        async fn fetch(&self) -> Result<QuotaUsage, QuotaServiceError> {
            self.result.clone()
        }
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = router(
            Arc::new(StubProvider {
                result: Ok(QuotaUsage::default()),
            }),
            Arc::new(ShutdownSignal::default()),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            serde_json::json!({"status":"ok"})
        );
    }

    #[tokio::test]
    async fn quota_usage_returns_fixed_shape() {
        let app = router(
            Arc::new(StubProvider {
                result: Ok(QuotaUsage {
                    weekly: Some(59),
                    monthly: None,
                    extra: Some(83),
                    hourly: Some(12),
                    code_review: Some(7),
                }),
            }),
            Arc::new(ShutdownSignal::default()),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/quota/usage")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            serde_json::json!({
                "weekly": 59,
                "monthly": null,
                "extra": 83,
                "hourly": 12,
                "code_review": 7
            })
        );
    }

    #[tokio::test]
    async fn maps_auth_upstream_and_internal_errors() {
        let cases = [
            (
                QuotaServiceError::Auth(crate::auth::AuthLoadError::MissingAccessToken),
                StatusCode::UNAUTHORIZED,
            ),
            (
                QuotaServiceError::Upstream {
                    status: 429,
                    reason: "Too Many Requests".to_owned(),
                },
                StatusCode::BAD_GATEWAY,
            ),
            (
                QuotaServiceError::Internal("network".to_owned()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];
        for (error, expected) in cases {
            let app = router(
                Arc::new(StubProvider { result: Err(error) }),
                Arc::new(ShutdownSignal::default()),
            );
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/quota/usage")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
    }

    #[tokio::test]
    async fn shutdown_request_returns_accepted_and_sets_signal() {
        let signal = Arc::new(ShutdownSignal::default());
        let app = router(
            Arc::new(StubProvider {
                result: Ok(QuotaUsage::default()),
            }),
            Arc::clone(&signal),
        );
        let request = Request::builder()
            .method("POST")
            .uri("/control/shutdown")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            serde_json::json!({"status":"shutting_down"})
        );
        assert!(signal.is_requested());
    }
}
