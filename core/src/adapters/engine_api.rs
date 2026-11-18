//! Reads from a container engine's HTTP API on its Unix socket.
//!
//! Status and logs are asked for many times a minute, and every engine CLI
//! process costs tens of milliseconds of CPU on a small host. A read that
//! the API can answer starts no process at all. Only reads go through
//! here: everything that changes the engine still runs its CLI through the
//! supervised process runner.

use std::path::{Path, PathBuf};
use std::time::Duration;

use http_body_util::{BodyExt as _, Empty};
use hyper::body::{Bytes, Incoming};
use hyper_util::rt::TokioIo;

use crate::domain::error::MmError;

/// The longest answer read into memory at once.
const MAX_DOCUMENT_BYTES: usize = 32 * 1024 * 1024;
/// How long the engine may take to answer a read that is not a stream.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// One engine's API socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineApi {
    socket: PathBuf,
}

impl EngineApi {
    #[must_use]
    pub fn at(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
        }
    }

    /// The socket of the Docker engine the `docker` command of this process
    /// would talk to, when that is a Unix socket on this host. `None` when
    /// the environment or the Docker configuration names another engine (a
    /// TCP address, a context), and when no socket is there: the caller
    /// then asks the CLI, which follows those settings itself.
    #[must_use]
    pub fn local_docker() -> Option<Self> {
        Self::docker_from(
            |name| match std::env::var(name) {
                Ok(value) if !value.is_empty() => Some(value),
                _ => None,
            },
            Path::new("/var/run/docker.sock"),
        )
    }

    /// `local_docker` with the environment and the default socket given.
    #[must_use]
    pub fn docker_from(
        variable: impl Fn(&str) -> Option<String>,
        default_socket: &Path,
    ) -> Option<Self> {
        let socket = match variable("DOCKER_HOST") {
            Some(host) => PathBuf::from(host.strip_prefix("unix://")?),
            None => {
                if variable("DOCKER_CONTEXT").is_some_and(|context| context != "default")
                    || configured_context(&variable).is_some_and(|context| context != "default")
                {
                    return None;
                }
                default_socket.to_path_buf()
            }
        };
        is_socket(&socket).then_some(Self { socket })
    }

    #[must_use]
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// The JSON document at `path` (a path with its query string).
    pub async fn json(&self, path: &str) -> Result<serde_json::Value, MmError> {
        self.json_if_found(path)
            .await?
            .ok_or_else(|| MmError::RuntimeResponse {
                message: format!("the engine has nothing at {}", path_only(path)),
            })
    }

    /// The JSON document at `path`, or `None` when the engine says the
    /// thing asked for is not there (a container that was just removed).
    pub async fn json_if_found(&self, path: &str) -> Result<Option<serde_json::Value>, MmError> {
        let _phase = crate::app::timing::phase_of("engine", &path_only(path));
        let read = async {
            let Some(response) = self.get(path).await? else {
                return Ok(None);
            };
            let bytes = response
                .into_body()
                .collect()
                .await
                .map_err(|error| failed(path, &error))?
                .to_bytes();
            if bytes.len() > MAX_DOCUMENT_BYTES {
                return Err(MmError::RuntimeResponse {
                    message: format!(
                        "the engine's answer to {} is larger than {MAX_DOCUMENT_BYTES} bytes",
                        path_only(path)
                    ),
                });
            }
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| MmError::RuntimeResponse {
                    message: format!(
                        "the engine's answer to {} is not JSON: {error}",
                        path_only(path)
                    ),
                })
        };
        tokio::time::timeout(READ_TIMEOUT, read)
            .await
            .map_err(|_| MmError::RuntimeResponse {
                message: format!(
                    "the engine did not answer {} within {} seconds",
                    path_only(path),
                    READ_TIMEOUT.as_secs()
                ),
            })?
    }

    /// The answer at `path` as it arrives, for a log that is followed;
    /// `None` when the engine says the thing asked for is not there.
    pub async fn stream(&self, path: &str) -> Result<Option<EngineStream>, MmError> {
        let response = tokio::time::timeout(READ_TIMEOUT, self.get(path))
            .await
            .map_err(|_| MmError::RuntimeResponse {
                message: format!(
                    "the engine did not answer {} within {} seconds",
                    path_only(path),
                    READ_TIMEOUT.as_secs()
                ),
            })??;
        Ok(response.map(|response| EngineStream {
            path: path_only(path),
            body: response.into_body(),
        }))
    }

    /// One `GET` on a connection of its own; the answer's status is
    /// checked, and "not found" is `None`.
    async fn get(&self, path: &str) -> Result<Option<hyper::Response<Incoming>>, MmError> {
        let stream = tokio::net::UnixStream::connect(&self.socket)
            .await
            .map_err(|source| MmError::Io {
                context: format!("connecting to the engine at {}", self.socket.display()),
                source,
            })?;
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .map_err(|error| failed(path, &error))?;
        // The connection ends when the answer has been read or dropped.
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::debug!(%error, "an engine API connection ended with an error");
            }
        });
        let request = hyper::Request::builder()
            .method(hyper::Method::GET)
            .uri(path)
            .header(hyper::header::HOST, "engine")
            .body(Empty::<Bytes>::new())
            .map_err(|error| failed(path, &error))?;
        let response = sender
            .send_request(request)
            .await
            .map_err(|error| failed(path, &error))?;
        let status = response.status();
        if status.is_success() {
            return Ok(Some(response));
        }
        if status == hyper::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        // The engine says why in a small JSON document.
        let reason = match response.into_body().collect().await {
            Ok(body) => serde_json::from_slice::<serde_json::Value>(&body.to_bytes())
                .map(|document| document["message"].as_str().unwrap_or_default().to_owned())
                .unwrap_or_default(),
            Err(_) => String::new(),
        };
        Err(MmError::RuntimeResponse {
            message: format!(
                "the engine answered {} with {status}: {reason}",
                path_only(path)
            ),
        })
    }
}

/// An answer that is read piece by piece.
pub struct EngineStream {
    path: String,
    body: Incoming,
}

impl EngineStream {
    /// The next piece; `None` when the engine has ended the answer.
    pub async fn next(&mut self) -> Result<Option<Bytes>, MmError> {
        loop {
            match self.body.frame().await {
                None => return Ok(None),
                Some(Ok(frame)) => {
                    if let Ok(data) = frame.into_data() {
                        return Ok(Some(data));
                    }
                }
                Some(Err(error)) => return Err(failed(&self.path, &error)),
            }
        }
    }
}

/// Encodes `value` for use as one value of a query string.
#[must_use]
pub fn query_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// The `currentContext` of the Docker CLI's configuration file.
fn configured_context(variable: &impl Fn(&str) -> Option<String>) -> Option<String> {
    let directory = variable("DOCKER_CONFIG")
        .map(PathBuf::from)
        .or_else(|| variable("HOME").map(|home| Path::new(&home).join(".docker")))?;
    // No file, or one that cannot be read: the CLI uses its default too.
    let Ok(text) = std::fs::read_to_string(directory.join("config.json")) else {
        return None;
    };
    let Ok(document) = serde_json::from_str::<serde_json::Value>(&text) else {
        return None;
    };
    document["currentContext"]
        .as_str()
        .filter(|context| !context.is_empty())
        .map(str::to_owned)
}

fn is_socket(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt as _;
    std::fs::metadata(path).is_ok_and(|metadata| metadata.file_type().is_socket())
}

/// The path without its query string: what a message and a trace name.
fn path_only(path: &str) -> String {
    path.split('?').next().unwrap_or(path).to_owned()
}

fn failed(path: &str, error: &dyn std::fmt::Display) -> MmError {
    MmError::RuntimeResponse {
        message: format!(
            "reading {} from the engine failed: {error}",
            path_only(path)
        ),
    }
}
