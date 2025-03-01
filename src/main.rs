use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde_json::json;
use std::sync::{atomic::AtomicUsize, Arc};
use tokio::signal;

struct RoundRobin {
    hosts: Vec<String>,
    host_idx: AtomicUsize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let rr = RoundRobin {
        hosts: vec!["localhost:10241".to_string(), "localhost:10242".to_string()],
        host_idx: AtomicUsize::new(0),
    };

    tracing_subscriber::fmt::init();

    let state = Arc::new(rr);

    let router = Router::new()
        .route("/health", get(health))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("localhost:10240")
        .await
        .context(
            "the tcp listener couldn't be binded to the address localhost in the port 10240",
        )?;

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("couldn't initialize the tcp listener")?;

    Ok(())
}

struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

async fn health(state: State<Arc<RoundRobin>>) -> Result<Response<Body>, ApiError> {
    let host_idx = state
        .host_idx
        .fetch_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
            |v| Some(if v >= state.hosts.len() { 0 } else { v + 1 }),
        )
        .map_err(|e| anyhow::anyhow!(e))
        .context("fetching and updating the host_idx failed, falling back to index 0")?;

    let host = state
        .hosts
        .get(host_idx)
        .context(format!("the host with index={host_idx:?} is not available"))?;

    Ok((StatusCode::OK, Json(json!({"health": "ok", "host": host }))).into_response())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install ctrl+c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
