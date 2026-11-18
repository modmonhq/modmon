//! Frames of the remote protocol read in a loop that also waits for other
//! things: a frame that arrives in pieces is never lost.

use std::time::Duration;

use mm::adapters::remote::{FrameReader, encode_frame, read_raw_frame};
use tokio::io::AsyncWriteExt as _;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn framed(payload: &[u8]) -> Vec<u8> {
    let mut bytes = u32::try_from(payload.len())
        .unwrap_or(0)
        .to_be_bytes()
        .to_vec();
    bytes.extend_from_slice(payload);
    bytes
}

/// A log line of 200 kB travels in pieces. A reader that is dropped while
/// it waits for the rest (the client looks every 25 ms whether the read
/// was cancelled) must find the frame whole afterwards.
#[tokio::test]
async fn a_frame_that_arrives_in_pieces_survives_a_reader_that_gives_up_between_them() -> TestResult
{
    let long_line = serde_json::json!({"message": "x".repeat(200_000)});
    let first = encode_frame(&long_line)?;
    let second = encode_frame(&serde_json::json!({"message": "after"}))?;
    let mut wire = framed(&first);
    wire.extend(framed(&second));

    let (mut writer, reader) = tokio::io::duplex(64 * 1024);
    let sending = tokio::spawn(async move {
        for piece in wire.chunks(7_000) {
            if writer.write_all(piece).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(3)).await;
        }
    });
    let mut frames = FrameReader::new(reader);
    let mut read = Vec::new();
    let mut gave_up = 0;
    while read.len() < 2 {
        // Waiting only two milliseconds at a time gives up in the middle
        // of the frame many times over.
        match tokio::time::timeout(Duration::from_millis(2), frames.next_raw()).await {
            Ok(frame) => read.push(frame?.ok_or("the stream ended early")?),
            Err(_) => gave_up += 1,
        }
    }
    sending.await?;
    assert!(gave_up > 3, "the read was never interrupted ({gave_up})");
    assert_eq!(read[0], first);
    assert_eq!(read[1], second);
    assert_eq!(frames.next_raw().await?, None);
    Ok(())
}

/// What went wrong before: the two-step read loses what it read when it is
/// dropped, and then takes payload bytes for a length.
#[tokio::test]
async fn the_two_step_read_loses_a_frame_when_it_is_dropped_half_way() -> TestResult {
    let payload = vec![b'x'; 50_000];
    let wire = framed(&payload);
    let (mut writer, mut reader) = tokio::io::duplex(64 * 1024);
    writer.write_all(&wire[..20_000]).await?;
    // The length and part of the payload are there; the read waits for
    // the rest and is dropped.
    let waited = tokio::time::timeout(Duration::from_millis(20), read_raw_frame(&mut reader)).await;
    assert!(waited.is_err());
    writer.write_all(&wire[20_000..]).await?;
    drop(writer);
    let error = read_raw_frame(&mut reader)
        .await
        .err()
        .ok_or("the broken read went unnoticed")?;
    assert!(
        error
            .to_string()
            .contains("frame length must be between 1 and 8388608 bytes"),
        "{error}"
    );
    Ok(())
}

#[tokio::test]
async fn a_stream_that_ends_inside_a_frame_is_an_error_and_an_empty_frame_is_refused() -> TestResult
{
    let (mut writer, reader) = tokio::io::duplex(1024);
    writer.write_all(&framed(b"{}")[..3]).await?;
    drop(writer);
    let error = FrameReader::new(reader)
        .next_raw()
        .await
        .err()
        .ok_or("a cut frame was accepted")?;
    assert!(
        error.to_string().contains("ended inside a frame"),
        "{error}"
    );

    let (mut writer, reader) = tokio::io::duplex(1024);
    writer.write_all(&0_u32.to_be_bytes()).await?;
    let error = FrameReader::new(reader)
        .next_raw()
        .await
        .err()
        .ok_or("an empty frame was accepted")?;
    assert!(
        error.to_string().contains("frame length must be between"),
        "{error}"
    );
    Ok(())
}
