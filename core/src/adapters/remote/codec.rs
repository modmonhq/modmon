use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::domain::error::MmError;
use crate::domain::remote::MAX_FRAME_BYTES;

pub async fn read_raw_frame<R>(reader: &mut R) -> Result<Option<Vec<u8>>, MmError>
where
    R: AsyncRead + Unpin + ?Sized,
{
    let mut length = [0_u8; 4];
    let mut read = 0;
    while read < length.len() {
        let count = reader
            .read(&mut length[read..])
            .await
            .map_err(|source| protocol_io("reading frame length", source))?;
        if count == 0 {
            if read == 0 {
                return Ok(None);
            }
            return Err(protocol("truncated frame length"));
        }
        read += count;
    }
    let length = usize::try_from(u32::from_be_bytes(length))
        .map_err(|_| protocol("frame length is not supported on this platform"))?;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(protocol(&format!(
            "frame length must be between 1 and {MAX_FRAME_BYTES} bytes"
        )));
    }
    let mut bytes = vec![0_u8; length];
    reader
        .read_exact(&mut bytes)
        .await
        .map_err(|source| protocol_io("reading frame payload", source))?;
    Ok(Some(bytes))
}

/// Reads frames from one stream and keeps what it has read of a frame
/// between calls.
///
/// `read_raw_frame` reads a frame in two steps, so a caller that gives up
/// in the middle (a `select!` whose other branch wins) loses the bytes it
/// read, and the next read begins inside a frame: it takes payload bytes
/// for a length and fails with "frame length must be between 1 and …"
/// (a log stream of long lines, whose frames arrive in pieces, read
/// in a loop that also wakes for other things). Here `next` can be dropped
/// at any point and called again: nothing is lost.
pub struct FrameReader<R> {
    reader: R,
    /// The bytes read and not yet returned: at most one frame and the
    /// beginning of the next.
    buffered: Vec<u8>,
}

impl<R> FrameReader<R>
where
    R: AsyncRead + Unpin,
{
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffered: Vec::new(),
        }
    }

    /// The next frame's payload, or `None` when the stream ended between
    /// frames.
    pub async fn next_raw(&mut self) -> Result<Option<Vec<u8>>, MmError> {
        loop {
            if let Some(header) = self.buffered.first_chunk::<4>() {
                let length = usize::try_from(u32::from_be_bytes(*header))
                    .map_err(|_| protocol("frame length is not supported on this platform"))?;
                if length == 0 || length > MAX_FRAME_BYTES {
                    return Err(protocol(&format!(
                        "frame length must be between 1 and {MAX_FRAME_BYTES} bytes"
                    )));
                }
                if self.buffered.len() >= 4 + length {
                    let rest = self.buffered.split_off(4 + length);
                    let frame = self.buffered.split_off(4);
                    self.buffered = rest;
                    return Ok(Some(frame));
                }
            }
            // One read is all or nothing when it is dropped.
            let mut chunk = [0_u8; 16 * 1024];
            let count = self
                .reader
                .read(&mut chunk)
                .await
                .map_err(|source| protocol_io("reading a frame", source))?;
            if count == 0 {
                return if self.buffered.is_empty() {
                    Ok(None)
                } else {
                    Err(protocol("the stream ended inside a frame"))
                };
            }
            self.buffered.extend_from_slice(&chunk[..count]);
        }
    }

    /// The next frame, decoded.
    pub async fn next<T: DeserializeOwned>(&mut self) -> Result<Option<T>, MmError> {
        self.next_raw()
            .await?
            .map(|bytes| decode_frame(&bytes))
            .transpose()
    }
}

pub async fn write_raw_frame<W>(writer: &mut W, bytes: &[u8]) -> Result<(), MmError>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(protocol("outbound frame exceeds the protocol bound"));
    }
    let length = u32::try_from(bytes.len())
        .map_err(|_| protocol("outbound frame length is not representable"))?;
    writer
        .write_all(&length.to_be_bytes())
        .await
        .map_err(|source| protocol_io("writing framed response", source))?;
    writer
        .write_all(bytes)
        .await
        .map_err(|source| protocol_io("writing framed response", source))?;
    writer
        .flush()
        .await
        .map_err(|source| protocol_io("flushing framed response", source))
}

pub fn encode_frame<T: Serialize>(frame: &T) -> Result<Vec<u8>, MmError> {
    let bytes = serde_json::to_vec(frame).map_err(|error| MmError::RuntimeResponse {
        message: format!("could not encode remote protocol frame: {error}"),
    })?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(protocol("encoded frame exceeds the protocol bound"));
    }
    Ok(bytes)
}

pub fn decode_frame<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, MmError> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(protocol("inbound frame exceeds the protocol bound"));
    }
    serde_json::from_slice(bytes).map_err(|error| MmError::RuntimeResponse {
        message: format!("remote protocol frame is malformed or unsupported: {error}"),
    })
}

pub async fn write_frame<W, T>(writer: &mut W, frame: &T) -> Result<usize, MmError>
where
    W: AsyncWrite + Unpin + ?Sized,
    T: Serialize,
{
    let bytes = encode_frame(frame)?;
    let length = bytes.len();
    write_raw_frame(writer, &bytes).await?;
    Ok(length)
}

/// One frame, decoded, for tests that read a whole frame at a time.
#[cfg(test)]
pub async fn read_frame<R, T>(reader: &mut R) -> Result<Option<T>, MmError>
where
    R: AsyncRead + Unpin + ?Sized,
    T: DeserializeOwned,
{
    read_raw_frame(reader)
        .await?
        .map(|bytes| decode_frame(&bytes))
        .transpose()
}

fn protocol(message: &str) -> MmError {
    MmError::RuntimeResponse {
        message: format!("remote protocol error: {message}"),
    }
}

fn protocol_io(context: &str, source: std::io::Error) -> MmError {
    MmError::Io {
        context: format!("{context} for remote protocol"),
        source,
    }
}
