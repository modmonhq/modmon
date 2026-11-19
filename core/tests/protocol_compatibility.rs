//! Clients and hosts on different releases: the remote protocol they agree
//! on, what a daemon makes of a newer or an older client, what a client
//! makes of what a newer daemon adds, and the frozen protocol-19 frames
//! every later build still reads and writes the same.
//!
//! The fixtures under `tests/fixtures/protocol/<N>/` are written once by
//! `cargo test --test protocol_compatibility -- --ignored write_protocol_fixtures`
//! and never rewritten: a change that breaks one breaks the peers that
//! still speak protocol N.

mod daemon_host;
mod temp_dirs;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use daemon_host::{ALICE, Host, PROMPT, TestResult};
use mm::adapters::remote::decode_frame;
use mm::domain::remote::{
    Behind, ClientFrame, MINIMUM_PROTOCOL_VERSION, PROTOCOL_VERSION, ProtocolRange, RemoteMethod,
    ServerFrame, UPGRADE_REQUIRED_CODE, negotiate, protocol_refusal, require_protocol,
};
use serde_json::{Map, Value, json};

const THIS: &str = env!("CARGO_PKG_VERSION");
const NEWER: &str = "9.9.9";

fn range(minimum: u16, maximum: u16) -> ProtocolRange {
    ProtocolRange { minimum, maximum }
}

#[test]
fn client_and_daemon_speak_the_newest_protocol_both_have() {
    let base = MINIMUM_PROTOCOL_VERSION;
    let cases = [
        (range(base, base + 6), range(base, base), Ok(base)),
        (range(base, base), range(base, base + 6), Ok(base)),
        (
            range(base, base + 6),
            range(base + 1, base + 3),
            Ok(base + 3),
        ),
        (
            range(base + 2, base + 2),
            range(base, base + 6),
            Ok(base + 2),
        ),
        (
            range(base + 2, base + 6),
            range(base, base + 1),
            Err(Behind::Daemon),
        ),
        (
            range(base - 1, base - 1),
            range(base, base + 6),
            Err(Behind::Client),
        ),
        (
            range(base + 3, base + 1),
            range(base, base + 6),
            Err(Behind::Daemon),
        ),
    ];
    for (client, daemon, agreed) in cases {
        assert_eq!(negotiate(client, daemon), agreed, "{client} with {daemon}");
    }
    assert_eq!(
        ProtocolRange::THIS.to_string(),
        PROTOCOL_VERSION.to_string()
    );
}

#[test]
fn a_request_that_needs_a_newer_protocol_fails_before_it_is_sent() {
    let host = ("node 'app-1' (203.0.113.11:22)", "0.1.1");
    let needed = PROTOCOL_VERSION + 1;
    let refused = require_protocol(PROTOCOL_VERSION, needed, host, "`teleport`")
        .err()
        .map(|error| (error.code(), error.to_string()));
    assert_eq!(
        refused,
        Some((
            UPGRADE_REQUIRED_CODE,
            "node 'app-1' (203.0.113.11:22) runs mmd 0.1.1, which does not support `teleport`; upgrade it with `mm target setup plan --upgrade` and apply that plan with --upgrade".to_owned()
        ))
    );
    assert!(require_protocol(needed, needed, host, "`teleport`").is_ok());
    for method in [
        RemoteMethod::Status,
        RemoteMethod::WhoAmI,
        RemoteMethod::Node,
    ] {
        assert!(
            method.protocol() <= MINIMUM_PROTOCOL_VERSION,
            "{} needs a protocol not every host this mm speaks to has",
            method.name()
        );
    }
}

fn hello(host: &Host, (version, minimum): (u16, Option<u16>), release: &str) -> Value {
    let mut hello = json!({
        "type": "hello",
        "version": version,
        "hello": {
            "client_nonce": uuid::Uuid::new_v4(),
            "expected_daemon_fingerprint": host.daemon.identity().fingerprint,
            "client_version": release,
            "added_by_a_newer_mm": {"anything": [1, 2]},
        },
        "added_by_a_newer_mm": true,
    });
    if let Some(minimum) = minimum {
        hello["minimum"] = json!(minimum);
    }
    hello
}

fn request(version: u16, method: Value) -> TestResult<(Value, uuid::Uuid)> {
    let mut frame = serde_json::to_value(Host::request(
        RemoteMethod::WhoAmI,
        Duration::from_secs(60),
    )?)?;
    frame["version"] = json!(version);
    frame["method"] = method;
    let request_id = serde_json::from_value(frame["request_id"].clone())?;
    Ok((frame, request_id))
}

#[tokio::test]
async fn a_daemon_answers_a_newer_client_in_the_protocol_both_speak() -> TestResult {
    let host = Host::with_alice().await?;
    let newer = PROTOCOL_VERSION + 6;
    let mut client = host.session(ALICE).await?;
    client
        .send_json(&hello(
            &host,
            (newer, Some(MINIMUM_PROTOCOL_VERSION)),
            NEWER,
        ))
        .await?;
    let (whoami, request_id) = request(newer, json!({"method": "who_am_i"}))?;
    client.send_json(&whoami).await?;
    let answer = client.next_json(PROMPT).await?;
    assert_eq!(answer["type"], "hello", "{answer}");
    assert_eq!(answer["version"], PROTOCOL_VERSION, "{answer}");
    assert_eq!(answer["hello"]["daemon_version"], THIS, "{answer}");
    let result = client.next_json(PROMPT).await?;
    assert_eq!(result["type"], "result", "{result}");
    assert_eq!(result["version"], PROTOCOL_VERSION, "{result}");
    assert_eq!(result["request_id"], json!(request_id), "{result}");
    assert_eq!(result["result"]["result"], "identity", "{result}");
    Ok(())
}

#[tokio::test]
async fn a_daemon_refuses_a_client_it_has_no_protocol_in_common_with() -> TestResult {
    let host = Host::with_alice().await?;
    let cases = [
        (
            (PROTOCOL_VERSION + 11, Some(PROTOCOL_VERSION + 6)),
            NEWER,
            protocol_refusal(
                (NEWER, range(PROTOCOL_VERSION + 6, PROTOCOL_VERSION + 11)),
                (THIS, ProtocolRange::THIS),
            ),
            "Upgrade the host: run `mm target setup plan --upgrade` and apply that plan with --upgrade",
        ),
        (
            (MINIMUM_PROTOCOL_VERSION - 1, None),
            "",
            protocol_refusal(
                (
                    "",
                    range(MINIMUM_PROTOCOL_VERSION - 1, MINIMUM_PROTOCOL_VERSION - 1),
                ),
                (THIS, ProtocolRange::THIS),
            ),
            "Update mm on this machine to",
        ),
    ];
    for (offer, release, refusal, step) in cases {
        let mut client = host.session(ALICE).await?;
        client.send_json(&hello(&host, offer, release)).await?;
        let (code, message) = client.refusal(PROMPT).await?;
        assert_eq!(code, UPGRADE_REQUIRED_CODE);
        assert_eq!(message, refusal);
        assert!(message.contains(step), "{message}");
        assert!(
            message.contains(&format!(
                "the host's mmd {THIS} speaks {}",
                ProtocolRange::THIS
            )),
            "{message}"
        );
        assert!(client.ended().await?.is_err());
    }
    Ok(())
}

#[tokio::test]
async fn a_daemon_answers_a_request_it_cannot_read_and_acts_on_none_of_it() -> TestResult {
    let host = Host::with_alice().await?;
    let newer = PROTOCOL_VERSION + 6;
    let unknown_method = json!({"method": "teleport", "payload": {"to": "elsewhere"}});
    let (mut unknown_field, _) = request(newer, json!({"method": "who_am_i"}))?;
    unknown_field["on_behalf_of"] = json!("bob");
    let (unknown_method, _) = request(newer, unknown_method)?;
    for (frame, method) in [(unknown_method, "teleport"), (unknown_field, "who_am_i")] {
        let mut client = host.session(ALICE).await?;
        client
            .send_json(&hello(
                &host,
                (newer, Some(MINIMUM_PROTOCOL_VERSION)),
                NEWER,
            ))
            .await?;
        assert_eq!(client.next_json(PROMPT).await?["type"], "hello");
        client.send_json(&frame).await?;
        let answer = client.next_json(PROMPT).await?;
        assert_eq!(answer["type"], "error", "{answer}");
        assert_eq!(answer["version"], PROTOCOL_VERSION, "{answer}");
        assert_eq!(answer["request_id"], frame["request_id"], "{answer}");
        assert_eq!(answer["error"]["code"], UPGRADE_REQUIRED_CODE, "{answer}");
        assert_eq!(
            answer["error"]["message"],
            format!(
                "mmd {THIS} on this host does not understand `{method}` from mm {NEWER}. Upgrade the host: run `mm target setup plan --upgrade` and apply that plan with --upgrade"
            ),
            "{answer}"
        );
        assert_eq!(answer["error"]["mutation_state"], "not_started", "{answer}");
        assert!(client.ended().await?.is_ok());
    }
    assert!(
        !host
            .audit()?
            .iter()
            .any(|line| line["result"] == "authorized"),
        "a request the daemon could not read was authorized"
    );

    let (mut same, _) = request(PROTOCOL_VERSION, json!({"method": "who_am_i"}))?;
    same["on_behalf_of"] = json!("bob");
    let mut client = host.session(ALICE).await?;
    client
        .send_json(&hello(
            &host,
            (PROTOCOL_VERSION, Some(MINIMUM_PROTOCOL_VERSION)),
            THIS,
        ))
        .await?;
    assert_eq!(client.next_json(PROMPT).await?["type"], "hello");
    client.send_json(&same).await?;
    let (code, message) = client.refusal(PROMPT).await?;
    assert_eq!(code, "MM_RUNTIME_RESPONSE", "{message}");
    assert!(
        message.contains("unknown field `on_behalf_of`"),
        "{message}"
    );
    assert!(client.ended().await?.is_err());
    Ok(())
}

#[tokio::test]
async fn a_relay_passes_on_what_a_newer_client_and_daemon_add() -> TestResult {
    use mm::adapters::remote::{read_raw_frame, relay_session, write_raw_frame};

    let (mut client, mut relay_stdin) = tokio::io::duplex(64 * 1024);
    let (mut relay_stdout, mut client_reads) = tokio::io::duplex(64 * 1024);
    let (mut daemon, mut relay_socket) = tokio::io::duplex(64 * 1024);
    let (mut relay_writes, mut daemon_reads) = tokio::io::duplex(64 * 1024);
    let relay = tokio::spawn(async move {
        relay_session(
            (&mut relay_stdin, &mut relay_stdout),
            (&mut relay_socket, &mut relay_writes),
            Duration::from_secs(10),
        )
        .await
    });
    let from_client = serde_json::to_vec(&json!({
        "type": "teleport",
        "version": PROTOCOL_VERSION + 6,
        "to": "elsewhere",
    }))?;
    write_raw_frame(&mut client, &from_client).await?;
    assert_eq!(read_raw_frame(&mut daemon_reads).await?, Some(from_client));
    let progress = serde_json::to_vec(&json!({
        "type": "progress",
        "version": PROTOCOL_VERSION,
        "done": 3,
    }))?;
    for _ in 0..2 {
        write_raw_frame(&mut daemon, &progress).await?;
    }
    for _ in 0..2 {
        assert_eq!(
            read_raw_frame(&mut client_reads).await?,
            Some(progress.clone())
        );
    }
    let other = serde_json::to_vec(&json!({"type": "end", "version": PROTOCOL_VERSION + 1}))?;
    write_raw_frame(&mut daemon, &other).await?;
    assert!(
        tokio::time::timeout(PROMPT, relay).await??.is_err(),
        "a daemon frame of another protocol than the session's was relayed"
    );
    Ok(())
}

#[test]
fn a_client_reads_what_a_newer_daemon_adds_to_its_answers() -> TestResult {
    let root = fixtures(PROTOCOL_VERSION);
    let added = json!({"added_by_a_newer_mmd": [1, {"deep": true}]});
    let answers = [
        ("server/hello.json", "/hello"),
        ("server/error.json", "/error"),
        ("server/end.json", ""),
        ("server/stream/log.json", ""),
        ("server/result/identity.json", "/result/value"),
        ("server/result/node.json", "/result/value"),
        ("server/result/node.json", "/result/value/daemon"),
        ("server/result/mesh_status.json", "/result/value"),
        ("server/result/networks.json", "/result/value"),
        ("server/result/deployment_status.json", "/result/value"),
        ("server/result/storage_status.json", "/result/value"),
        ("server/result/runtimes.json", "/result/value"),
        ("server/result/volume_deleted.json", "/result/value"),
        ("server/result/delivered.json", "/result/value"),
        ("server/result/cancelled.json", "/result/value"),
        ("server/result/staged.json", "/result/value"),
        ("server/result/host_values.json", "/result/value"),
        ("server/result/status.json", "/result"),
    ];
    for (fixture, at) in answers {
        let frozen: Value = serde_json::from_str(&std::fs::read_to_string(root.join(fixture))?)?;
        let mut newer = frozen.clone();
        let Some(Value::Object(object)) = newer.pointer_mut(at) else {
            return Err(format!("{fixture} has no object at {at:?}").into());
        };
        object.insert("added_by_a_newer_mmd".to_owned(), added.clone());
        newer["added_by_a_newer_mmd"] = added.clone();
        let read: ServerFrame = decode_frame(&serde_json::to_vec(&newer)?)
            .map_err(|error| format!("{fixture} at {at:?}: {error}"))?;
        assert_eq!(serde_json::to_value(read)?, frozen, "{fixture} at {at:?}");
    }
    Ok(())
}

#[test]
fn every_object_a_request_carries_is_read_strictly() -> TestResult {
    let (schema, defs) = schema_of::<ClientFrame>()?;
    let request = branches(&schema)
        .into_iter()
        .find(|(kind, _)| kind == "request")
        .map(|(_, branch)| branch)
        .ok_or("no request frame")?;
    let mut seen = BTreeSet::new();
    let mut open = Vec::new();
    open_objects(&request, &defs, ("request", &mut seen), &mut open);
    assert!(
        open.is_empty(),
        "a daemon would ignore what a newer client adds to these: {open:?}"
    );
    Ok(())
}

/// Objects that accept fields they do not know. A build receipt is not
/// looked into: its hash covers everything in it.
fn open_objects(
    schema: &Value,
    defs: &Map<String, Value>,
    (at, seen): (&str, &mut BTreeSet<String>),
    open: &mut Vec<String>,
) {
    match schema {
        Value::Object(object) => {
            if let Some(name) = object
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|reference| reference.rsplit('/').next())
            {
                if name != "BuildReceipt"
                    && seen.insert(name.to_owned())
                    && let Some(def) = defs.get(name)
                {
                    open_objects(def, defs, (name, seen), open);
                }
                return;
            }
            if object.contains_key("properties")
                && object.get("additionalProperties") != Some(&Value::Bool(false))
            {
                open.push(at.to_owned());
            }
            for (key, inner) in object {
                let inner_at = if key == "properties" {
                    at.to_owned()
                } else {
                    format!("{at}.{key}")
                };
                open_objects(inner, defs, (&inner_at, seen), open);
            }
        }
        Value::Array(items) => {
            for item in items {
                open_objects(item, defs, (at, seen), open);
            }
        }
        _ => {}
    }
}

#[test]
fn frozen_protocol_frames_still_read_and_write_the_same() -> TestResult {
    let mut problems = Vec::new();
    let mut checked = 0;
    for version in ProtocolRange::THIS.minimum..=ProtocolRange::THIS.maximum {
        let root = fixtures(version);
        if !root.is_dir() {
            problems.push(format!(
                "{} is missing: run `cargo test --test protocol_compatibility -- --ignored write_protocol_fixtures`",
                root.display()
            ));
            continue;
        }
        for (side, read) in [("client", read_client as Reader), ("server", read_server)] {
            for path in files(&root.join(side))? {
                checked += 1;
                if let Err(problem) = round_trip(&path, read) {
                    problems.push(format!("{}: {problem}", path.display()));
                }
            }
        }
    }
    for expected in expected_fixtures()? {
        if !fixtures(PROTOCOL_VERSION).join(&expected).is_file() {
            problems.push(format!(
                "protocol {PROTOCOL_VERSION} has no fixture {expected}: a new frame, method or result needs a new protocol and its fixtures"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(checked > 60, "only {checked} frozen frames were read");
    Ok(())
}

type Reader = fn(&[u8]) -> Result<Value, String>;

fn read_client(bytes: &[u8]) -> Result<Value, String> {
    let frame: ClientFrame = decode_frame(bytes).map_err(|error| error.to_string())?;
    serde_json::to_value(frame).map_err(|error| error.to_string())
}

fn read_server(bytes: &[u8]) -> Result<Value, String> {
    let frame: ServerFrame = decode_frame(bytes).map_err(|error| error.to_string())?;
    serde_json::to_value(frame).map_err(|error| error.to_string())
}

fn round_trip(path: &Path, read: Reader) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let frozen: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let written = read(text.as_bytes())?;
    if written != frozen {
        return Err(format!("this build writes it as {written}"));
    }
    Ok(())
}

fn fixtures(version: u16) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/protocol")
        .join(version.to_string())
}

fn files(directory: &Path) -> TestResult<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            found.extend(files(&path)?);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// Every frame, method, result and stream item this build has, as the
/// fixture path that freezes it.
fn expected_fixtures() -> TestResult<Vec<String>> {
    Ok(samples()?.into_iter().map(|(path, _)| path).collect())
}

#[test]
#[ignore = "writes the fixtures of a new protocol once; run it by name"]
fn write_protocol_fixtures() -> TestResult {
    let root = fixtures(PROTOCOL_VERSION);
    let mut problems = Vec::new();
    for (path, frame) in samples()? {
        let read = if path.starts_with("client/") {
            read_client
        } else {
            read_server
        };
        let canonical = match read(&serde_json::to_vec(&frame)?) {
            Ok(canonical) => canonical,
            Err(problem) => {
                problems.push(format!("{path}: {problem}\n{frame}"));
                continue;
            }
        };
        let target = root.join(&path);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, serde_json::to_string_pretty(&canonical)? + "\n")?;
    }
    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
    Ok(())
}

/// A sample of every frame this build writes: each client frame, each
/// request method, each server frame, each result and each stream item,
/// with every field filled.
fn samples() -> TestResult<Vec<(String, Value)>> {
    let mut samples = Vec::new();
    for (side, (schema, defs)) in [
        ("client", schema_of::<ClientFrame>()?),
        ("server", schema_of::<ServerFrame>()?),
    ] {
        let sampler = Sampler { defs: &defs };
        for (kind, branch) in branches(&schema) {
            let mut frame = sampler.sample(&branch, 0);
            frame["version"] = json!(PROTOCOL_VERSION);
            match (side, kind.as_str()) {
                ("client", "hello") => {
                    frame["minimum"] = json!(MINIMUM_PROTOCOL_VERSION);
                    frame["hello"]["client_version"] = json!(THIS);
                }
                ("server", "hello") => frame["hello"]["daemon_version"] = json!(THIS),
                _ => {}
            }
            let variants = match (side, kind.as_str()) {
                ("client", "request") => Some(("method", "RemoteMethod")),
                ("server", "result") => Some(("result", "RemoteResult")),
                ("server", "stream") => Some(("item", "RemoteStreamItem")),
                _ => None,
            };
            let Some((field, def)) = variants else {
                samples.push((format!("{side}/{kind}.json"), frame));
                continue;
            };
            let enumeration = defs
                .get(def)
                .ok_or_else(|| format!("no {def} in the schema"))?;
            for (variant, schema) in branches(enumeration) {
                let mut frame = frame.clone();
                frame[field] = sampler.sample(&schema, 1);
                let path = format!("{side}/{kind}/{variant}.json");
                if path == "client/request/exec.json" {
                    frame["method"]["payload"]["timeout"] = json!({"secs": 30, "nanos": 0});
                }
                samples.push((path, frame));
            }
        }
    }
    Ok(samples)
}

fn schema_of<T: schemars::JsonSchema>() -> TestResult<(Value, Map<String, Value>)> {
    let mut schema = serde_json::to_value(schemars::schema_for!(T))?;
    let defs = match schema.as_object_mut().and_then(|root| root.remove("$defs")) {
        Some(Value::Object(defs)) => defs,
        _ => Map::new(),
    };
    Ok((schema, defs))
}

/// The branches of a tagged enum's schema, each with its tag.
fn branches(schema: &Value) -> Vec<(String, Value)> {
    let Some(Value::Array(branches)) = schema.get("oneOf") else {
        return Vec::new();
    };
    branches
        .iter()
        .filter_map(|branch| {
            let tag = match branch.get("const").or_else(|| branch.pointer("/enum/0")) {
                Some(tag) => tag.as_str()?.to_owned(),
                None => branch
                    .get("properties")?
                    .as_object()?
                    .values()
                    .find_map(|property| property.get("const")?.as_str())?
                    .to_owned(),
            };
            Some((tag, branch.clone()))
        })
        .collect()
}

/// A value of a schema with every field it may have filled in.
struct Sampler<'a> {
    defs: &'a Map<String, Value>,
}

const DEEPEST: usize = 12;

impl Sampler<'_> {
    fn sample(&self, schema: &Value, depth: usize) -> Value {
        let Some(schema) = schema.as_object() else {
            return Value::Null;
        };
        if let Some(name) = schema
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| reference.rsplit('/').next())
        {
            return self
                .defs
                .get(name)
                .map_or(Value::Null, |def| self.sample(def, depth + 1));
        }
        if let Some(value) = schema.get("const") {
            return value.clone();
        }
        if let Some(Value::Array(values)) = schema.get("enum") {
            return values.first().cloned().unwrap_or(Value::Null);
        }
        let branch = ["oneOf", "anyOf"].iter().find_map(|key| {
            let Some(Value::Array(branches)) = schema.get(*key) else {
                return None;
            };
            branches
                .iter()
                .find(|branch| branch.get("type") != Some(&json!("null")))
                .or_else(|| branches.first())
        });
        let own = match kind(schema) {
            Some("object") => self.object(schema, depth),
            Some("array") => match schema.get("items") {
                Some(items) if depth < DEEPEST => json!([self.sample(items, depth + 1)]),
                _ => json!([]),
            },
            Some("string") => string(schema),
            Some("integer") => json!(
                schema
                    .get("minimum")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .max(1)
            ),
            Some("number") => json!(1.5),
            Some("boolean") => json!(true),
            _ => Value::Null,
        };
        match (branch.map(|branch| self.sample(branch, depth)), own) {
            (Some(Value::Object(mut chosen)), Value::Object(own)) => {
                chosen.extend(own);
                Value::Object(chosen)
            }
            (Some(chosen), Value::Null) => chosen,
            (_, own) => own,
        }
    }

    fn object(&self, schema: &Map<String, Value>, depth: usize) -> Value {
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|names| {
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let mut object = Map::new();
        if let Some(Value::Object(properties)) = schema.get("properties") {
            for (name, property) in properties {
                if depth < DEEPEST || required.contains(name.as_str()) {
                    object.insert(name.clone(), self.sample(property, depth + 1));
                }
            }
        }
        if let Some(values @ Value::Object(_)) = schema.get("additionalProperties")
            && object.is_empty()
            && depth < DEEPEST
        {
            object.insert("web".to_owned(), self.sample(values, depth + 1));
        }
        Value::Object(object)
    }
}

fn kind(schema: &Map<String, Value>) -> Option<&str> {
    match schema.get("type") {
        Some(Value::String(kind)) => Some(kind),
        Some(Value::Array(kinds)) => kinds
            .iter()
            .filter_map(Value::as_str)
            .find(|kind| *kind != "null"),
        _ if schema.contains_key("properties") => Some("object"),
        _ => None,
    }
}

fn string(schema: &Map<String, Value>) -> Value {
    let format = schema.get("format").and_then(Value::as_str);
    let pattern = schema
        .get("pattern")
        .and_then(Value::as_str)
        .unwrap_or_default();
    json!(match format {
        Some("uuid") => "00000000-0000-4000-8000-000000000001",
        Some("ipv4") => "10.0.0.1",
        Some("date-time") => "2026-11-18T12:00:00Z",
        _ if pattern.contains("://") => "https://web.example.test/",
        _ if pattern.contains("/\\d") => "10.0.0.0/24",
        _ => "web",
    })
}
