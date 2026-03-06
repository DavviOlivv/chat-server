use axum::body::Body;
use axum::http::Request;
use axum::http::{header::CONTENT_TYPE, StatusCode};
use axum::{response::IntoResponse, routing::get, Json, Router};
use once_cell::sync::Lazy;
use prometheus::{gather, register_counter, Counter, Encoder, TextEncoder};
use serde_json::json;
use std::net::SocketAddr;
use tracing::warn;

async fn health_handler() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

async fn metrics_handler(req: Request<Body>) -> impl IntoResponse {
    // If METRICS_BEARER_TOKEN is set, require Authorization: Bearer <token>
    if let Ok(expected) = std::env::var("METRICS_BEARER_TOKEN") {
        match req.headers().get("authorization") {
            Some(header_val) => {
                if let Ok(s) = header_val.to_str() {
                    let expected_header = format!("Bearer {}", expected);
                    if s != expected_header {
                        METRICS_UNAUTHORIZED_TOTAL.inc();
                        warn!("Unauthorized /metrics access: invalid bearer");
                        return (
                            StatusCode::UNAUTHORIZED,
                            [(CONTENT_TYPE, "text/plain; charset=utf-8")],
                            "unauthorized".to_string(),
                        );
                    }
                } else {
                    METRICS_UNAUTHORIZED_TOTAL.inc();
                    warn!("Unauthorized /metrics access: invalid header encoding");
                    return (
                        StatusCode::UNAUTHORIZED,
                        [(CONTENT_TYPE, "text/plain; charset=utf-8")],
                        "unauthorized".to_string(),
                    );
                }
            }
            None => {
                METRICS_UNAUTHORIZED_TOTAL.inc();
                warn!("Unauthorized /metrics access: missing Authorization header");
                return (
                    StatusCode::UNAUTHORIZED,
                    [(CONTENT_TYPE, "text/plain; charset=utf-8")],
                    "unauthorized".to_string(),
                );
            }
        }
    }

    let encoder = TextEncoder::new();
    let metric_families = gather();
    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        let body = format!("failed to encode metrics: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(CONTENT_TYPE, "text/plain; charset=utf-8")],
            body,
        );
    }
    let body = String::from_utf8(buffer).unwrap_or_default();
    // TextEncoder format type is typically: "text/plain; version=0.0.4; charset=utf-8"
    (
        StatusCode::OK,
        [(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
}

// Prometheus metric for unauthorized attempts to /metrics
static METRICS_UNAUTHORIZED_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "chat_metrics_unauthorized_total",
        "Total de tentativas não autorizadas ao endpoint /metrics"
    )
    .unwrap()
});

pub async fn run_metrics_server(
    bind: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/health", get(health_handler));
    let addr: SocketAddr = bind.parse()?;
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}
