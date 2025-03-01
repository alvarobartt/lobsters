use anyhow::{Context, Result};
use axum::{extract::State, response::Json, routing::get, Router};
use serde_json::{json, Value};
use std::sync::{atomic::AtomicUsize, Arc};
use tokio::signal;

enum Algos {
    RoundRobin {
        #[allow(dead_code)]
        hosts: Vec<String>,
        host_idx: AtomicUsize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let algo = Algos::RoundRobin {
        hosts: vec!["localhost:10241".to_string(), "localhost:10242".to_string()],
        host_idx: AtomicUsize::new(0),
    };

    match algo {
        Algos::RoundRobin { hosts: _, host_idx } => {
            tracing_subscriber::fmt::init();

            let state = Arc::new(host_idx);

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
        }
    }

    Ok(())
}

async fn health(state: State<Arc<AtomicUsize>>) -> Json<Value> {
    let host = state
        .fetch_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
            |v| Some(if v >= 2 { 0 } else { v + 1 }),
        )
        .unwrap_or(0);
    Json(json!({"health": "ok", "host": host }))
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
