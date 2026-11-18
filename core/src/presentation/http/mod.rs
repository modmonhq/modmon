use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Form, OriginalUri, Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE, HOST, ORIGIN, SET_COOKIE};
use axum::http::uri::Authority;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::stream::{self, Stream, StreamExt};
use rust_embed::RustEmbed;

use crate::app::ports::DashboardSubscription;
use crate::app::{DashboardQueryService, GateManager};
use crate::domain::dashboard::{DashboardEvent, DashboardScope};
use crate::domain::error::MmError;

const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; font-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";
const INGRESS_AUTH_HEADER: &str = "x-mm-ingress-auth";

#[derive(RustEmbed)]
#[folder = "ui/dist/"]
pub(super) struct Assets;

#[derive(Clone)]
pub(super) struct HttpState {
    pub(super) queries: DashboardQueryService,
    pub(super) events: Arc<dyn DashboardSubscription>,
    pub(super) ingress_token_hash: blake3::Hash,
}

#[derive(Clone)]
pub(super) struct GateHttpState {
    pub(super) ingress_token_hash: blake3::Hash,
    pub(super) gates: GateManager,
}

mod dashboard;
mod gate;
mod security;

use dashboard::*;
use gate::*;
use security::*;

pub(crate) async fn serve(
    address: SocketAddr,
    ingress_token: String,
    queries: DashboardQueryService,
    events: Arc<dyn DashboardSubscription>,
    gates: GateManager,
) -> Result<(), MmError> {
    if address.port() == 0 {
        return Err(MmError::Security {
            message: "the dashboard backend must bind a non-zero authenticated control port"
                .to_owned(),
        });
    }
    if ingress_token.len() != 64
        || ingress_token
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !matches!(byte, b'a'..=b'f'))
    {
        return Err(MmError::Security {
            message: "the dashboard ingress credential is invalid".to_owned(),
        });
    }
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|source| MmError::Io {
            context: format!("binding dashboard backend to {address}"),
            source,
        })?;
    let ingress_token_hash = blake3::hash(ingress_token.as_bytes());
    let dashboard_state = HttpState {
        queries,
        events,
        ingress_token_hash,
    };
    let gate_state = GateHttpState {
        ingress_token_hash,
        gates,
    };
    let dashboard_router = Router::new()
        .route("/api/v1/dashboard", get(dashboard))
        .route("/api/v1/events", get(event_stream))
        .route("/api/v1/schema", get(schema))
        .fallback(static_asset)
        .with_state(dashboard_state);
    let gate_router = Router::new()
        .route("/_mm/gate/forward/{policy_id}", get(gate_forward))
        .route(
            "/_mm/gate/login/{policy_id}",
            get(gate_login_page).post(gate_login),
        )
        .route("/_mm/gate/client.js", get(gate_client_script))
        .route("/_mm/gate/assets/{name}", get(gate_asset))
        .route("/_mm/gate/logout/{policy_id}", post(gate_logout))
        .with_state(gate_state);
    let router = dashboard_router.merge(gate_router);
    axum::serve(listener, router)
        .await
        .map_err(|source| MmError::Io {
            context: "serving the local dashboard".to_owned(),
            source,
        })
}

#[cfg(test)]
mod tests;
