use super::*;

pub(super) async fn dashboard(State(state): State<HttpState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_ingress(&headers, &state.ingress_token_hash) {
        return rejected(response);
    }
    let scope = match request_scope(&headers) {
        Ok(scope) => scope,
        Err(response) => return rejected(response),
    };
    if let Err(response) = require_safe_origin(&headers) {
        return rejected(response);
    }
    match state.queries.query(scope).await {
        Ok(value) => secured((StatusCode::OK, axum::Json(value)).into_response(), false),
        Err(error) => api_error(error),
    }
}

pub(super) async fn schema(State(state): State<HttpState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_ingress(&headers, &state.ingress_token_hash) {
        return rejected(response);
    }
    let scope = match request_scope(&headers) {
        Ok(scope) => scope,
        Err(response) => return rejected(response),
    };
    if let Err(error) = state.queries.authorize_scope(&scope) {
        return api_error(error);
    }
    if let Err(response) = require_safe_origin(&headers) {
        return rejected(response);
    }
    let value = serde_json::json!({
        "api_version": crate::domain::dashboard::DASHBOARD_API_VERSION,
        "dashboard": schemars::schema_for!(crate::domain::dashboard::DashboardResponse),
        "event": schemars::schema_for!(crate::domain::dashboard::DashboardEvent),
        "runtime_status": schemars::schema_for!(crate::domain::workspace::WorkspaceSnapshot),
        "log_request": schemars::schema_for!(crate::domain::observability::LogRequest),
        "log_record": schemars::schema_for!(crate::domain::observability::LogRecord),
        "exec_request": schemars::schema_for!(crate::domain::observability::ExecRequest),
        "exec_event": schemars::schema_for!(crate::domain::observability::ExecEvent),
        "job_run": schemars::schema_for!(crate::domain::scheduler::JobRun),
        "schedule": schemars::schema_for!(crate::domain::scheduler::ScheduleRecord),
        "scheduler_plan": schemars::schema_for!(crate::domain::scheduler::ReconciliationPlan),
    });
    secured((StatusCode::OK, axum::Json(value)).into_response(), false)
}

pub(super) async fn event_stream(State(state): State<HttpState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_ingress(&headers, &state.ingress_token_hash) {
        return rejected(response);
    }
    let scope = match request_scope(&headers) {
        Ok(scope) => scope,
        Err(response) => return rejected(response),
    };
    if let Err(error) = state.queries.authorize_scope(&scope) {
        return api_error(error);
    }
    if let Err(response) = require_safe_origin(&headers) {
        return rejected(response);
    }
    let connected = state.events.connected_event();
    let receiver = state.events.subscribe();
    let initial = stream::once(async move { event_result(connected) });
    let updates = stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(event) => return Some((event_result(event), receiver)),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    let stream: std::pin::Pin<
        Box<dyn Stream<Item = Result<Event, std::convert::Infallible>> + Send>,
    > = Box::pin(initial.chain(updates));
    secured(
        Sse::new(stream)
            .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
            .into_response(),
        false,
    )
}

pub(super) fn event_result(event: DashboardEvent) -> Result<Event, std::convert::Infallible> {
    let event_type = match &event {
        DashboardEvent::Connected { .. } => "connected",
        DashboardEvent::Refresh { .. } => "refresh",
    };
    let data = serde_json::to_string(&event).unwrap_or_else(|_| {
        "{\"type\":\"refresh\",\"sequence\":0,\"occurred_at_unix_ms\":0}".to_owned()
    });
    Ok(Event::default().event(event_type).data(data))
}

pub(super) async fn static_asset(
    State(state): State<HttpState>,
    headers: HeaderMap,
    method: Method,
    OriginalUri(uri): OriginalUri,
) -> Response {
    if let Err(response) = require_ingress(&headers, &state.ingress_token_hash) {
        return rejected(response);
    }
    if !matches!(method, Method::GET | Method::HEAD) {
        return secured(StatusCode::METHOD_NOT_ALLOWED.into_response(), false);
    }
    let scope = match request_scope(&headers) {
        Ok(scope) => scope,
        Err(response) => return rejected(response),
    };
    if let Err(error) = state.queries.authorize_scope(&scope) {
        return api_error(error);
    }
    if let Err(response) = require_safe_origin(&headers) {
        return rejected(response);
    }
    let requested = uri.path().trim_start_matches('/');
    let asset_name = if requested.is_empty() {
        "index.html"
    } else {
        requested
    };
    let asset = Assets::get(asset_name).or_else(|| {
        (!asset_name.contains('.'))
            .then(|| Assets::get("index.html"))
            .flatten()
    });
    let Some(asset) = asset else {
        return secured(StatusCode::NOT_FOUND.into_response(), false);
    };
    let content_type = match asset_name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json; charset=utf-8",
        Some("woff2") => "font/woff2",
        _ => "text/html; charset=utf-8",
    };
    let immutable = asset_name.starts_with("assets/") && asset_name.contains('-');
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(asset.data.into_owned())
    };
    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    secured(response, immutable)
}
