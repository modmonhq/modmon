//! The one request a metrics listener answers: `GET /metrics`, with an
//! optional bearer token.

/// A request for the metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsRequest {
    /// The bearer token of its `Authorization` header.
    pub bearer: Option<String>,
    /// Whether it asked for the head only.
    pub head: bool,
}

/// Why a request gets no metrics, as the HTTP status it is answered with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestProblem {
    BadRequest,
    NotFound,
    MethodNotAllowed,
}

impl RequestProblem {
    #[must_use]
    pub const fn status(self) -> &'static str {
        match self {
            Self::BadRequest => "400 Bad Request",
            Self::NotFound => "404 Not Found",
            Self::MethodNotAllowed => "405 Method Not Allowed",
        }
    }
}

/// Reads a request's line and headers, up to the empty line.
pub fn parse_request(head: &str) -> Result<MetricsRequest, RequestProblem> {
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(target), Some(version), None) = (
        request_line.next(),
        request_line.next(),
        request_line.next(),
        request_line.next(),
    ) else {
        return Err(RequestProblem::BadRequest);
    };
    if !version.starts_with("HTTP/1.") {
        return Err(RequestProblem::BadRequest);
    }
    let path = target.split_once('?').map_or(target, |(path, _)| path);
    if path != "/metrics" {
        return Err(RequestProblem::NotFound);
    }
    let head = match method {
        "GET" => false,
        "HEAD" => true,
        _ => return Err(RequestProblem::MethodNotAllowed),
    };
    let mut bearer = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("authorization") {
            let value = value.trim();
            bearer = value
                .split_once(' ')
                .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
                .map(|(_, token)| token.trim().to_owned());
        }
    }
    Ok(MetricsRequest { bearer, head })
}
