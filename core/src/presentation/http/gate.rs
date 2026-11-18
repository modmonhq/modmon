use super::*;

pub(super) async fn gate_client_script(
    State(state): State<GateHttpState>,
    headers: HeaderMap,
) -> Response {
    if require_ingress(&headers, &state.ingress_token_hash).is_err()
        || require_forwarded_https(&headers).is_err()
    {
        return gate_rejected(StatusCode::FORBIDDEN);
    }
    let source = "const s=document.currentScript;window.CAP_CSS_NONCE=s?.dataset.styleNonce||'';window.CAP_CUSTOM_WASM_URL='/_mm/gate/assets/cap_wasm_bg.wasm';window.CAP_PAKO_URL='/_mm/gate/assets/pako.min.js';const ready=()=>{const w=document.querySelector('cap-widget');const t=document.querySelector('#cap-token');const f=document.querySelector('form');if(w&&t&&f){w.addEventListener('solve',e=>{t.value=e.detail.token});f.addEventListener('submit',e=>{if(!t.value)e.preventDefault()})}};document.addEventListener('DOMContentLoaded',ready,{once:true});";
    let mut response = Response::new(Body::from(source));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/javascript; charset=utf-8"),
    );
    gate_secured(response)
}

pub(super) async fn gate_asset(
    State(state): State<GateHttpState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Response {
    if require_ingress(&headers, &state.ingress_token_hash).is_err()
        || require_forwarded_https(&headers).is_err()
    {
        return gate_rejected(StatusCode::FORBIDDEN);
    }
    let content_type = match name.as_str() {
        "widget.js" | "pako.min.js" => "text/javascript; charset=utf-8",
        "cap_wasm_bg.wasm" => "application/wasm",
        _ => return gate_rejected(StatusCode::NOT_FOUND),
    };
    let Some(asset) = Assets::get(&format!("gate/{name}")) else {
        return gate_rejected(StatusCode::NOT_FOUND);
    };
    let mut response = Response::new(Body::from(asset.data));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    let mut response = gate_secured(response);
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=0, must-revalidate"),
    );
    response
}

pub(super) async fn gate_forward(
    State(state): State<GateHttpState>,
    Path(policy_id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> Response {
    if require_ingress(&headers, &state.ingress_token_hash).is_err()
        || require_forwarded_https(&headers).is_err()
    {
        return gate_rejected(StatusCode::FORBIDDEN);
    }
    let cookie_name = crate::domain::gate::gate_session_cookie_name(policy_id);
    if let Some(cookie) = cookie_value(&headers, &cookie_name)
        && let Ok(validation) = state.gates.validate_session(policy_id, cookie).await
    {
        let mut response = gate_secured(StatusCode::OK.into_response());
        if validation.requires_rotation {
            let Ok(rotation) = state.gates.rotate_validated_session(&validation).await else {
                return gate_login_redirect(&state.gates, policy_id, &headers);
            };
            let max_age = (rotation
                .session
                .expires_at_unix_ms
                .saturating_sub(rotation.session.issued_at_unix_ms)
                / 1000)
                .max(0);
            append_cookie(
                &mut response,
                gate_session_cookie(&cookie_name, rotation.cookie_value.as_str(), max_age),
            );
        }
        if let Ok(value) = HeaderValue::from_str(&validation.principal.normalized_username) {
            response.headers_mut().insert("x-mm-gate-principal", value);
        }
        return response;
    }
    gate_login_redirect(&state.gates, policy_id, &headers)
}

#[derive(Default, serde::Deserialize)]
pub(super) struct GateLoginPageQuery {
    pub(super) return_to: Option<String>,
}

pub(super) async fn gate_login_page(
    State(state): State<GateHttpState>,
    Path(policy_id): Path<uuid::Uuid>,
    Query(query): Query<GateLoginPageQuery>,
    headers: HeaderMap,
) -> Response {
    if require_ingress(&headers, &state.ingress_token_hash).is_err()
        || require_forwarded_https(&headers).is_err()
    {
        return gate_rejected(StatusCode::FORBIDDEN);
    }
    gate_login_response(
        &state.gates,
        policy_id,
        &headers,
        query.return_to.as_deref(),
    )
}

#[derive(serde::Deserialize)]
pub(super) struct GateLoginForm {
    pub(super) username: String,
    pub(super) password: zeroize::Zeroizing<String>,
    pub(super) cap_token: zeroize::Zeroizing<String>,
    pub(super) csrf_token: zeroize::Zeroizing<String>,
    pub(super) return_to: String,
}

pub(super) async fn gate_login(
    State(state): State<GateHttpState>,
    Path(policy_id): Path<uuid::Uuid>,
    headers: HeaderMap,
    Form(form): Form<GateLoginForm>,
) -> Response {
    if require_ingress(&headers, &state.ingress_token_hash).is_err()
        || require_forwarded_https(&headers).is_err()
        || require_exact_origin(&headers).is_err()
        || !valid_return_path(&form.return_to)
    {
        return gate_rejected(StatusCode::FORBIDDEN);
    }
    let csrf_cookie_name = crate::domain::gate::gate_csrf_cookie_name(policy_id);
    let supplied_cookie = cookie_value(&headers, &csrf_cookie_name).unwrap_or_default();
    if !constant_time_equal(supplied_cookie.as_bytes(), form.csrf_token.as_bytes()) {
        return gate_rejected(StatusCode::FORBIDDEN);
    }
    let password = match crate::domain::secret::SecretMaterial::from_zeroizing(form.password) {
        Ok(value) => value,
        Err(_) => {
            return gate_login_response(&state.gates, policy_id, &headers, Some(&form.return_to));
        }
    };
    let cap_token = match crate::domain::secret::SecretMaterial::from_zeroizing(form.cap_token) {
        Ok(value) => value,
        Err(_) => {
            return gate_login_response(&state.gates, policy_id, &headers, Some(&form.return_to));
        }
    };
    let cookie_name = crate::domain::gate::gate_session_cookie_name(policy_id);
    let prior = if let Some(value) = cookie_value(&headers, &cookie_name) {
        match state.gates.validate_session(policy_id, value).await {
            Ok(validation) => Some(validation),
            Err(MmError::AuthenticationFailed) => None,
            Err(_) => return gate_rejected(StatusCode::SERVICE_UNAVAILABLE),
        }
    } else {
        None
    };
    match state
        .gates
        .login(crate::app::GateLoginRequest {
            policy_id,
            username: form.username,
            password,
            cap_token,
            replace_session_id: prior.as_ref().map(|value| value.session.id),
        })
        .await
    {
        Ok(login) => {
            let max_age = (login
                .session
                .expires_at_unix_ms
                .saturating_sub(login.session.issued_at_unix_ms)
                / 1000)
                .max(0);
            let mut response = gate_secured(StatusCode::SEE_OTHER.into_response());
            if let Ok(value) = HeaderValue::from_str(&form.return_to) {
                response.headers_mut().insert("location", value);
            }
            append_cookie(
                &mut response,
                gate_session_cookie(&cookie_name, login.cookie_value.as_str(), max_age),
            );
            append_cookie(
                &mut response,
                gate_csrf_cookie(&csrf_cookie_name, login.csrf_token.as_str(), max_age),
            );
            response
        }
        Err(error) => {
            tracing::warn!(
                policy_id = %policy_id,
                error_code = error.code(),
                error = %error,
                "gate login failed"
            );
            gate_login_response(&state.gates, policy_id, &headers, Some(&form.return_to))
        }
    }
}

#[derive(serde::Deserialize)]
pub(super) struct GateLogoutForm {
    pub(super) csrf_token: zeroize::Zeroizing<String>,
}

pub(super) async fn gate_logout(
    State(state): State<GateHttpState>,
    Path(policy_id): Path<uuid::Uuid>,
    headers: HeaderMap,
    Form(form): Form<GateLogoutForm>,
) -> Response {
    if require_ingress(&headers, &state.ingress_token_hash).is_err()
        || require_forwarded_https(&headers).is_err()
        || require_exact_origin(&headers).is_err()
    {
        return gate_rejected(StatusCode::FORBIDDEN);
    }
    let cookie_name = crate::domain::gate::gate_session_cookie_name(policy_id);
    let Some(cookie) = cookie_value(&headers, &cookie_name) else {
        return expired_gate_cookie_response(policy_id);
    };
    let Ok(validation) = state.gates.validate_session(policy_id, cookie).await else {
        return expired_gate_cookie_response(policy_id);
    };
    let csrf_cookie_name = crate::domain::gate::gate_csrf_cookie_name(policy_id);
    let supplied_cookie = cookie_value(&headers, &csrf_cookie_name).unwrap_or_default();
    if !constant_time_equal(supplied_cookie.as_bytes(), form.csrf_token.as_bytes()) {
        return gate_rejected(StatusCode::FORBIDDEN);
    }
    let binding = blake3::hash(form.csrf_token.as_bytes())
        .to_hex()
        .to_string();
    if !constant_time_equal(binding.as_bytes(), validation.csrf_binding.as_bytes()) {
        return gate_rejected(StatusCode::FORBIDDEN);
    }
    if state
        .gates
        .revoke_authenticated_session(validation.session.id)
        .is_err()
    {
        return gate_rejected(StatusCode::INTERNAL_SERVER_ERROR);
    }
    expired_gate_cookie_response(policy_id)
}

pub(super) fn gate_login_redirect(
    gates: &GateManager,
    policy_id: uuid::Uuid,
    headers: &HeaderMap,
) -> Response {
    if gates.policy_by_id(policy_id).is_err() {
        return gate_rejected(StatusCode::NOT_FOUND);
    }
    let query = query_value(&forwarded_return_path(headers));
    let Some(host) = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(HOST))
        .and_then(header_text)
    else {
        return gate_rejected(StatusCode::FORBIDDEN);
    };
    let mut response = gate_secured(StatusCode::SEE_OTHER.into_response());
    if let Ok(location) = HeaderValue::from_str(&format!(
        "https://{host}/_mm/gate/login/{policy_id}?return_to={query}"
    )) {
        response.headers_mut().insert("location", location);
    }
    response
}

pub(super) fn gate_login_response(
    gates: &GateManager,
    policy_id: uuid::Uuid,
    headers: &HeaderMap,
    return_to: Option<&str>,
) -> Response {
    let Ok(policy) = gates.policy_by_id(policy_id) else {
        return gate_rejected(StatusCode::NOT_FOUND);
    };
    let csrf = random_web_token();
    let style_nonce = random_web_token();
    if csrf.is_empty() || style_nonce.is_empty() {
        return gate_rejected(StatusCode::SERVICE_UNAVAILABLE);
    }
    let return_to = return_to
        .filter(|value| valid_return_path(value))
        .map_or_else(|| forwarded_return_path(headers), str::to_owned);
    let cap_endpoint = format!("https://cap.mm.test/{}/", html_escape(&policy.cap_site_key));
    let html = format!(
        "<!doctype html><html><head><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>Access required</title><script src=\"/_mm/gate/client.js\" data-style-nonce=\"{}\"></script><script src=\"/_mm/gate/assets/widget.js\"></script></head><body><main><h1>Access required</h1><p>This route is protected by {}.</p><form method=post action=\"/_mm/gate/login/{policy_id}\"><input type=hidden name=csrf_token value=\"{}\"><input type=hidden name=return_to value=\"{}\"><label>Username <input name=username autocomplete=username required maxlength=128></label><label>Password <input name=password type=password autocomplete=current-password required></label><cap-widget data-cap-worker-count=\"2\" data-cap-api-endpoint=\"{cap_endpoint}\"></cap-widget><input type=hidden name=cap_token id=cap-token><button type=submit>Continue</button></form></main></body></html>",
        html_escape(&style_nonce),
        html_escape(policy.name.as_str()),
        html_escape(&csrf),
        html_escape(&return_to)
    );
    let mut response = Response::new(Body::from(html));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    append_cookie(
        &mut response,
        format!(
            "{}={csrf}; Path=/; Max-Age=600; Secure; SameSite=Strict",
            crate::domain::gate::gate_csrf_cookie_name(policy_id)
        ),
    );
    gate_secured_with_style_nonce(response, Some(&style_nonce))
}

pub(super) fn expired_gate_cookie_response(policy_id: uuid::Uuid) -> Response {
    let mut response = gate_secured(StatusCode::SEE_OTHER.into_response());
    response
        .headers_mut()
        .insert("location", HeaderValue::from_static("/"));
    append_cookie(
        &mut response,
        format!(
            "{}=; Path=/; Max-Age=0; Secure; HttpOnly; SameSite=Lax",
            crate::domain::gate::gate_session_cookie_name(policy_id)
        ),
    );
    append_cookie(
        &mut response,
        format!(
            "{}=; Path=/; Max-Age=0; Secure; SameSite=Strict",
            crate::domain::gate::gate_csrf_cookie_name(policy_id)
        ),
    );
    response
}

pub(super) fn gate_rejected(status: StatusCode) -> Response {
    gate_secured(status.into_response())
}

pub(super) fn gate_secured(response: Response) -> Response {
    gate_secured_with_style_nonce(response, None)
}

pub(super) fn gate_secured_with_style_nonce(
    mut response: Response,
    style_nonce: Option<&str>,
) -> Response {
    let headers = response.headers_mut();
    let style_source = style_nonce.map_or_else(
        || "style-src 'none'".to_owned(),
        |nonce| format!("style-src 'nonce-{nonce}'"),
    );
    if let Ok(value) = HeaderValue::from_str(&format!(
        "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self' https://cap.mm.test; worker-src blob:; img-src data:; {style_source}; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
    )) {
        headers.insert("content-security-policy", value);
    }
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("same-origin"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub(super) fn append_cookie(response: &mut Response, value: String) {
    if let Ok(value) = HeaderValue::from_str(&value) {
        response.headers_mut().append(SET_COOKIE, value);
    }
}

pub(super) fn gate_session_cookie(name: &str, value: &str, max_age: i64) -> String {
    format!("{name}={value}; Path=/; Max-Age={max_age}; Secure; HttpOnly; SameSite=Lax")
}

pub(super) fn gate_csrf_cookie(name: &str, value: &str, max_age: i64) -> String {
    format!("{name}={value}; Path=/; Max-Age={max_age}; Secure; SameSite=Strict")
}

pub(super) fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(header_text)
        .flat_map(|value| value.split(';'))
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(key, value)| (key == name).then_some(value))
}

pub(super) fn require_forwarded_https(headers: &HeaderMap) -> Result<(), ()> {
    (headers.get("x-forwarded-proto").and_then(header_text) == Some("https")
        && headers.contains_key("x-forwarded-host"))
    .then_some(())
    .ok_or(())
}

pub(super) fn forwarded_return_path(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-uri")
        .and_then(header_text)
        .filter(|value| valid_return_path(value))
        .unwrap_or("/")
        .to_owned()
}

pub(super) fn valid_return_path(value: &str) -> bool {
    value.starts_with('/')
        && !value.starts_with("//")
        && !value.contains('\r')
        && !value.contains('\n')
}

pub(super) fn query_value(value: &str) -> String {
    value.bytes().fold(String::new(), |mut encoded, byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
        encoded
    })
}

pub(super) fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    use subtle::ConstantTimeEq as _;

    left.ct_eq(right).into()
}

pub(super) fn random_web_token() -> String {
    use base64::Engine as _;
    let mut bytes = [0_u8; 32];
    if getrandom::fill(&mut bytes).is_err() {
        return String::new();
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub(super) fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
