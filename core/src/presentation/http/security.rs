use super::*;

pub(super) fn request_scope(headers: &HeaderMap) -> Result<DashboardScope, StatusCode> {
    let value = headers
        .get(HOST)
        .and_then(header_text)
        .ok_or_else(misdirected)?;
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(misdirected());
    }
    let authority = Authority::from_str(value).map_err(|_| misdirected())?;
    let host = authority.host().trim_end_matches('.').to_ascii_lowercase();
    if host == "mm.test" {
        return Ok(DashboardScope::Installation);
    }
    let slug = host
        .strip_prefix("mm.")
        .and_then(|host| host.strip_suffix(".test"))
        .filter(|slug| !slug.is_empty() && !slug.contains('.'))
        .ok_or_else(misdirected)?;
    let normalized =
        crate::domain::local_ingress::ProjectSlug::normalize(slug).map_err(|_| misdirected())?;
    if normalized.as_str() != slug {
        return Err(misdirected());
    }
    Ok(DashboardScope::Project(slug.to_owned()))
}

pub(super) fn require_ingress(
    headers: &HeaderMap,
    expected: &blake3::Hash,
) -> Result<(), StatusCode> {
    let supplied = headers
        .get(INGRESS_AUTH_HEADER)
        .and_then(header_text)
        .ok_or_else(forbidden)?;
    let supplied = blake3::hash(supplied.as_bytes());
    let difference = supplied
        .as_bytes()
        .iter()
        .zip(expected.as_bytes())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        });
    if difference == 0 {
        Ok(())
    } else {
        Err(forbidden())
    }
}

pub(super) fn require_safe_origin(headers: &HeaderMap) -> Result<(), StatusCode> {
    let Some(origin) = headers.get(ORIGIN) else {
        return Ok(());
    };
    let origin = origin.to_str().map_err(|_| forbidden())?;
    let host = headers
        .get(HOST)
        .and_then(header_text)
        .ok_or_else(forbidden)?;
    if origin == format!("https://{host}") {
        Ok(())
    } else {
        Err(forbidden())
    }
}

pub(super) fn header_text(value: &HeaderValue) -> Option<&str> {
    match value.to_str() {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::debug!(%error, "ignoring HTTP header that is not visible ASCII");
            None
        }
    }
}

pub(super) fn require_exact_origin(headers: &HeaderMap) -> Result<(), StatusCode> {
    headers.get(ORIGIN).ok_or_else(forbidden)?;
    require_safe_origin(headers)
}

pub(super) fn api_error(error: MmError) -> Response {
    let status = match &error {
        MmError::Security { .. } => StatusCode::MISDIRECTED_REQUEST,
        MmError::ConfigNotFound { .. } => StatusCode::NOT_FOUND,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    let body = serde_json::json!({
        "error": {
            "code": error.code(),
            "message": "The local dashboard could not build a safe response. Run `mm doctor` for details."
        }
    });
    secured((status, axum::Json(body)).into_response(), false)
}

pub(super) fn misdirected() -> StatusCode {
    StatusCode::MISDIRECTED_REQUEST
}

pub(super) fn forbidden() -> StatusCode {
    StatusCode::FORBIDDEN
}

pub(super) fn rejected(status: StatusCode) -> Response {
    secured(status.into_response(), false)
}

pub(super) fn secured(mut response: Response, immutable: bool) -> Response {
    let headers = response.headers_mut();
    headers.insert("content-security-policy", HeaderValue::from_static(CSP));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "x-mm-csrf-required",
        HeaderValue::from_static("origin-and-token"),
    );
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static(if immutable {
            "public, max-age=31536000, immutable"
        } else {
            "no-store"
        }),
    );
    response
}

#[cfg(test)]
pub(super) fn future_mutation_guard(
    request: &axum::http::Request<Body>,
    reviewed: &str,
    current: &str,
    scope_authorized: bool,
) -> bool {
    let valid_hash =
        |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
    request.method() != Method::GET
        && request.method() != Method::HEAD
        && require_safe_origin(request.headers()).is_ok()
        && request
            .headers()
            .get("x-mm-csrf-token")
            .is_some_and(|token| !token.is_empty())
        && valid_hash(reviewed)
        && valid_hash(current)
        && reviewed == current
        && scope_authorized
}
