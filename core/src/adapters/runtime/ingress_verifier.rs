use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use async_trait::async_trait;

use crate::app::ports::IngressVerifier;
use crate::domain::error::MmError;
use crate::domain::traefik_dynamic::{TRAEFIK_VERIFY_HEADER, TRAEFIK_VERIFY_HEADER_VALUE};

const MAX_RESPONSE_HEADER_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone)]
pub struct TcpIngressVerifier {
    attempts: usize,
    retry_delay: Duration,
    timeout: Duration,
}

impl Default for TcpIngressVerifier {
    fn default() -> Self {
        Self {
            attempts: 20,
            retry_delay: Duration::from_millis(250),
            timeout: Duration::from_secs(5),
        }
    }
}

impl TcpIngressVerifier {
    #[must_use]
    pub const fn new(attempts: usize, retry_delay: Duration, timeout: Duration) -> Self {
        Self {
            attempts,
            retry_delay,
            timeout,
        }
    }

    async fn verify(&self, port: u16, host: &str, path: &str, route: bool) -> Result<(), MmError> {
        if self.attempts == 0 {
            return Err(MmError::RuntimeResponse {
                message: "ingress verification requires at least one attempt".to_owned(),
            });
        }
        if host.is_empty()
            || host.chars().any(char::is_control)
            || !path.starts_with('/')
            || path.chars().any(char::is_control)
        {
            return Err(MmError::Security {
                message: "refusing unsafe ingress verification request".to_owned(),
            });
        }

        let mut last_error = None;
        for attempt in 0..self.attempts {
            let host = host.to_owned();
            let path = path.to_owned();
            let timeout = self.timeout;
            let result = tokio::task::spawn_blocking(move || {
                verify_http_response(port, &host, &path, timeout, route)
            })
            .await
            .map_err(|_| MmError::RuntimeResponse {
                message: "ingress verification worker stopped unexpectedly".to_owned(),
            })?;
            match result {
                Ok(()) => return Ok(()),
                Err(error) => last_error = Some(error),
            }
            if attempt + 1 < self.attempts {
                tokio::time::sleep(self.retry_delay).await;
            }
        }
        Err(last_error.unwrap_or_else(|| MmError::RuntimeResponse {
            message: "ingress verification failed".to_owned(),
        }))
    }
}

#[async_trait]
impl IngressVerifier for TcpIngressVerifier {
    async fn verify_listener(&self, port: u16) -> Result<(), MmError> {
        self.verify(port, "mm.invalid", "/.mm-listener-check", false)
            .await
    }

    async fn verify_route(&self, port: u16, host: &str, path: &str) -> Result<(), MmError> {
        self.verify(port, host, path, true).await
    }
}

fn verify_http_response(
    port: u16,
    host: &str,
    path: &str,
    timeout: Duration,
    route: bool,
) -> Result<(), MmError> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, timeout).map_err(|source| {
        MmError::RuntimeResponse {
            message: format!("could not connect to loopback ingress port {port}: {source}"),
        }
    })?;
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|()| stream.set_write_timeout(Some(timeout)))
        .map_err(|source| MmError::Io {
            context: "configuring ingress verification socket".to_owned(),
            source,
        })?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: mm-ingress-verifier\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|source| MmError::Io {
            context: "writing ingress verification request".to_owned(),
            source,
        })?;
    let ResponseHead { status, headers } =
        read_final_headers(&mut stream, port, host, path, timeout)?;
    if route && matches!(status, 502..=504) {
        return Err(MmError::RuntimeResponse {
            message: format!(
                "loopback ingress could not reach the backend (HTTP {status}); route {host}{path} via 127.0.0.1:{port}"
            ),
        });
    }
    if route && !has_verification_header(&headers) {
        return Err(MmError::RuntimeResponse {
            message: format!(
                "loopback ingress did not match the reviewed route; {host}{path} via 127.0.0.1:{port}"
            ),
        });
    }
    Ok(())
}

struct ResponseHead {
    status: u16,
    headers: String,
}

fn read_final_headers(
    stream: &mut TcpStream,
    port: u16,
    host: &str,
    path: &str,
    timeout: Duration,
) -> Result<ResponseHead, MmError> {
    let deadline = std::time::Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| MmError::RuntimeResponse {
            message: "ingress verification timeout exceeds the supported range".to_owned(),
        })?;
    let timed_out = || MmError::RuntimeResponse {
        message: format!(
            "timed out after {}ms waiting for final HTTP headers from {host}{path} via 127.0.0.1:{port}; check the service's HTTP port and readiness",
            timeout.as_millis()
        ),
    };
    let mut response = Vec::with_capacity(1024);
    let mut received = 0;
    let mut chunk = [0_u8; 1024];
    loop {
        if let Some(end) = response.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = parse_response_head(&response[..end])?;
            if !(100..200).contains(&head.status) || head.status == 101 {
                return Ok(head);
            }
            // 103 Early Hints is not the backend response. Verification headers
            // and gateway failures belong to the final response that follows.
            response.drain(..end + 4);
            continue;
        }
        if received == MAX_RESPONSE_HEADER_BYTES {
            return Err(MmError::RuntimeResponse {
                message: "loopback ingress returned oversized HTTP headers".to_owned(),
            });
        }
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(timed_out)?;
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|source| MmError::Io {
                context: "setting ingress verification deadline".to_owned(),
                source,
            })?;
        let limit = (MAX_RESPONSE_HEADER_BYTES - received).min(chunk.len());
        let count = stream.read(&mut chunk[..limit]).map_err(|source| {
            if matches!(
                source.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                timed_out()
            } else {
                MmError::Io {
                    context: format!(
                        "reading ingress response for {host}{path} via 127.0.0.1:{port}"
                    ),
                    source,
                }
            }
        })?;
        if count == 0 {
            return Err(MmError::RuntimeResponse {
                message: "loopback ingress returned incomplete HTTP headers".to_owned(),
            });
        }
        received += count;
        response.extend_from_slice(&chunk[..count]);
    }
}

fn parse_response_head(bytes: &[u8]) -> Result<ResponseHead, MmError> {
    let headers = std::str::from_utf8(bytes).map_err(|_| MmError::RuntimeResponse {
        message: "loopback ingress returned non-UTF-8 HTTP headers".to_owned(),
    })?;
    let malformed = || MmError::RuntimeResponse {
        message: "loopback ingress returned a malformed HTTP status".to_owned(),
    };
    let status = headers
        .lines()
        .next()
        .filter(|line| line.starts_with("HTTP/1.0 ") || line.starts_with("HTTP/1.1 "))
        .and_then(|line| line.split_ascii_whitespace().nth(1))
        .ok_or_else(malformed)?
        .parse::<u16>()
        .map_err(|_| malformed())?;
    if !(100..600).contains(&status) {
        return Err(malformed());
    }
    Ok(ResponseHead {
        status,
        headers: headers.to_owned(),
    })
}

fn has_verification_header(headers: &str) -> bool {
    headers.lines().skip(1).any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case(TRAEFIK_VERIFY_HEADER)
                && value.trim() == TRAEFIK_VERIFY_HEADER_VALUE
        })
    })
}

#[cfg(test)]
#[path = "ingress_verifier/tests.rs"]
mod tests;
