use std::{env, time::Duration};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use serde::Serialize;
use tokio::{
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
struct AppState {
    valkey_address: String,
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    valkey: &'static str,
}

#[derive(Serialize)]
struct Quote {
    sku: String,
    currency: &'static str,
    amount_cents: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args().nth(1).as_deref() == Some("--healthcheck") {
        let port = env::var("PORT").unwrap_or_else(|_| "8080".to_owned());
        timeout(
            Duration::from_secs(2),
            TcpStream::connect(format!("127.0.0.1:{port}")),
        )
        .await??;
        return Ok(());
    }

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_owned());
    let valkey_host = env::var("VALKEY_HOST").unwrap_or_else(|_| "valkey".to_owned());
    let valkey_port = env::var("VALKEY_PORT").unwrap_or_else(|_| "6379".to_owned());
    let state = AppState {
        valkey_address: format!("{valkey_host}:{valkey_port}"),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/quotes/{sku}", get(quote))
        .with_state(state);

    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn health(State(state): State<AppState>) -> (StatusCode, Json<Health>) {
    let reachable = timeout(
        Duration::from_secs(1),
        TcpStream::connect(&state.valkey_address),
    )
    .await
    .is_ok_and(|result| result.is_ok());

    if reachable {
        (
            StatusCode::OK,
            Json(Health {
                status: "ok",
                valkey: "reachable",
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(Health {
                status: "degraded",
                valkey: "unreachable",
            }),
        )
    }
}

async fn quote(Path(sku): Path<String>) -> Json<Quote> {
    let checksum = sku
        .bytes()
        .fold(0_u32, |total, value| total + u32::from(value));

    Json(Quote {
        sku,
        currency: "USD",
        amount_cents: 1_000 + (checksum % 9_000),
    })
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let terminate = async {
            match signal(SignalKind::terminate()) {
                Ok(mut stream) => {
                    stream.recv().await;
                }
                Err(error) => {
                    tracing::warn!(%error, "could not install SIGTERM handler");
                    std::future::pending::<()>().await;
                }
            }
        };

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
