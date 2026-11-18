use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use super::{TRAEFIK_VERIFY_HEADER, verify_http_response};
use crate::app::ports::IngressVerifier as _;

#[test]
fn route_verification_requires_the_traefik_marker() -> Result<(), Box<dyn std::error::Error>> {
    let port = serve_once("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")?;
    let result = verify_http_response(
        port,
        "app.example.test",
        "/",
        std::time::Duration::from_secs(1),
        true,
    );
    assert!(result.is_err());
    Ok(())
}

#[test]
fn route_verification_accepts_a_marked_backend_response() -> Result<(), Box<dyn std::error::Error>>
{
    let response = format!(
        "HTTP/1.1 404 Not Found\r\n{TRAEFIK_VERIFY_HEADER}: 1\r\nContent-Length: 0\r\n\r\n"
    );
    let port = serve_once(&response)?;
    verify_http_response(
        port,
        "app.example.test",
        "/",
        std::time::Duration::from_secs(1),
        true,
    )?;
    Ok(())
}

#[test]
fn early_hints_are_skipped_and_only_the_final_response_is_verified()
-> Result<(), Box<dyn std::error::Error>> {
    for (final_headers, valid) in [
        (
            format!("HTTP/1.1 200 OK\r\n{TRAEFIK_VERIFY_HEADER}: 1\r\nContent-Length: 0\r\n\r\n"),
            true,
        ),
        (
            format!(
                "HTTP/1.1 502 Bad Gateway\r\n{TRAEFIK_VERIFY_HEADER}: 1\r\nContent-Length: 0\r\n\r\n"
            ),
            false,
        ),
        (
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_owned(),
            false,
        ),
    ] {
        let response = format!(
            "HTTP/1.1 103 Early Hints\r\nLink: </app.css>; rel=preload\r\n{TRAEFIK_VERIFY_HEADER}: 1\r\n\r\n{final_headers}"
        );
        let port = serve_once(&response)?;
        assert_eq!(
            verify_http_response(
                port,
                "web.store.test",
                "/",
                std::time::Duration::from_secs(1),
                true
            )
            .is_ok(),
            valid
        );
    }
    Ok(())
}

#[test]
fn route_verification_rejects_an_unreachable_marked_backend()
-> Result<(), Box<dyn std::error::Error>> {
    let response = format!(
        "HTTP/1.1 503 Service Unavailable\r\n{TRAEFIK_VERIFY_HEADER}: 1\r\nContent-Length: 0\r\n\r\n"
    );
    let port = serve_once(&response)?;
    let result = verify_http_response(
        port,
        "app.example.test",
        "/",
        std::time::Duration::from_secs(1),
        true,
    );
    assert!(result.is_err());
    Ok(())
}

#[test]
fn listener_verification_only_requires_an_http_response() -> Result<(), Box<dyn std::error::Error>>
{
    let port = serve_once("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")?;
    verify_http_response(
        port,
        "mm.invalid",
        "/.mm-listener-check",
        std::time::Duration::from_secs(1),
        false,
    )?;
    Ok(())
}

fn serve_once(response: &str) -> Result<u16, Box<dyn std::error::Error>> {
    serve_once_bytes(response.as_bytes().to_vec())
}

fn serve_once_bytes(response: Vec<u8>) -> Result<u16, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(&response);
        }
    });
    Ok(port)
}

#[tokio::test]
async fn verifier_rejects_unsafe_inputs_and_zero_attempts() {
    let verifier = super::TcpIngressVerifier::new(
        0,
        std::time::Duration::ZERO,
        std::time::Duration::from_millis(50),
    );
    assert!(verifier.verify_listener(80).await.is_err());
    let verifier = super::TcpIngressVerifier::new(
        1,
        std::time::Duration::ZERO,
        std::time::Duration::from_millis(50),
    );
    for (host, path) in [
        ("", "/"),
        ("bad\nhost", "/"),
        ("host", "relative"),
        ("host", "/bad\npath"),
    ] {
        assert!(verifier.verify_route(80, host, path).await.is_err());
    }
    let defaults = super::TcpIngressVerifier::default();
    assert_eq!(defaults.attempts, 20);
}

#[tokio::test]
async fn verifier_retries_then_accepts_listener_and_marked_route()
-> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    thread::spawn(move || {
        for response in [
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_owned(),
            format!("HTTP/1.1 200 OK\r\n{TRAEFIK_VERIFY_HEADER}: 1\r\nContent-Length: 0\r\n\r\n"),
        ] {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0_u8; 1024];
                let _ = stream.read(&mut request);
                let _ = stream.write_all(response.as_bytes());
            }
        }
    });
    let verifier = super::TcpIngressVerifier::new(
        2,
        std::time::Duration::from_millis(1),
        std::time::Duration::from_secs(1),
    );
    verifier
        .verify_route(port, "app.example.test", "/ready")
        .await?;

    let port = serve_once("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")?;
    verifier.verify_listener(port).await?;
    Ok(())
}

#[tokio::test]
async fn verifier_returns_last_connection_error_after_all_attempts()
-> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let verifier = super::TcpIngressVerifier::new(
        2,
        std::time::Duration::from_millis(1),
        std::time::Duration::from_millis(20),
    );
    assert!(verifier.verify_listener(port).await.is_err());
    Ok(())
}

#[test]
fn response_parser_rejects_bounded_malformed_and_non_utf8_headers()
-> Result<(), Box<dyn std::error::Error>> {
    for response in [
        b"incomplete".to_vec(),
        b"\r\n\r\n".to_vec(),
        b"garbage\r\n\r\n".to_vec(),
        b"HTTP/1.1 nope\r\n\r\n".to_vec(),
        vec![0xff, 0xfe, b'\r', b'\n', b'\r', b'\n'],
        vec![b'x'; super::MAX_RESPONSE_HEADER_BYTES],
    ] {
        let port = serve_once_bytes(response)?;
        assert!(
            verify_http_response(
                port,
                "app.example.test",
                "/",
                std::time::Duration::from_secs(1),
                false,
            )
            .is_err()
        );
    }
    Ok(())
}
