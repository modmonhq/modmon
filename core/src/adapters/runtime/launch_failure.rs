//! Why the engine could not start a service's or a job's container, in
//! mm's words, for the causes mm knows from what Docker, Podman, and
//! Compose print.

use std::collections::BTreeMap;

use crate::app::ports::CommandOutput;
use crate::domain::build::LOCAL_IMAGE_REGISTRY;
use crate::domain::error::MmError;
use crate::domain::runtime::RuntimeKind;

/// What the engine was to start.
#[derive(Debug, Clone, Copy)]
pub enum Launched<'a> {
    /// `compose up` of a stack's project, for `services`, or for every
    /// service when it names none.
    Services {
        stack: &'a str,
        project: &'a str,
        services: &'a [String],
    },
    /// The container of a run of `job`, made from `service`.
    Job { job: &'a str, service: &'a str },
}

/// The error of a launch that failed: [`MmError::Launch`] when [`explain`]
/// knows the cause, otherwise what `program` printed.
pub(crate) fn launch_error(
    runtime: RuntimeKind,
    program: String,
    launched: &Launched<'_>,
    output: &CommandOutput,
) -> MmError {
    explain(runtime, launched, &output.stderr).map_or_else(
        || MmError::ProcessFailed {
            program,
            code: output.code,
            stderr: output.stderr.trim().to_owned(),
        },
        |message| MmError::Launch { message },
    )
}

/// Why `launched` could not start, from what the engine printed on its
/// standard error; `None` for a cause mm does not know.
#[must_use]
pub fn explain(runtime: RuntimeKind, launched: &Launched<'_>, stderr: &str) -> Option<String> {
    let cause = Cause::of(stderr)?;
    let engine = match runtime {
        RuntimeKind::DockerCompose => "docker",
        RuntimeKind::PodmanCompose => "podman",
    };
    Some(format!(
        "{} could not start: {}",
        subject(launched, stderr),
        cause.words(engine, launched)
    ))
}

enum Cause {
    Mount {
        target: Option<String>,
        volume: Option<String>,
    },
    Image {
        image: Option<String>,
        pull: Pull,
    },
    Network(Option<String>),
    Port(Option<u16>),
    Name(Option<String>),
}

#[derive(Clone, Copy)]
enum Pull {
    Absent,
    Denied,
    Unknown,
    Unreachable,
    Limited,
    ShortName,
}

impl Cause {
    fn of(stderr: &str) -> Option<Self> {
        let lower = stderr.to_ascii_lowercase();
        if lower.contains("container name") && lower.contains("is already in use") {
            return Some(Self::Name(
                between(stderr, "container name \"", '"')
                    .map(|name| name.trim_start_matches('/'))
                    .filter(|name| plain(name))
                    .map(str::to_owned),
            ));
        }
        if lower.contains("port is already allocated") || lower.contains("address already in use") {
            return Some(Self::Port(port(stderr)));
        }
        if lower.contains("are you trying to mount a directory onto a file")
            || (lower.contains("not a directory")
                && (lower.contains("error mounting") || lower.contains("crun: mount")))
        {
            return Some(Self::Mount {
                target: mount_target(stderr),
                volume: mount_volume(stderr),
            });
        }
        if let Some(name) = missing_network(stderr) {
            return Some(Self::Network(name));
        }
        let pull = pull_failure(&lower)?;
        Some(Self::Image {
            image: image(stderr),
            pull,
        })
    }

    fn words(&self, engine: &str, launched: &Launched<'_>) -> String {
        let job = matches!(launched, Launched::Job { .. });
        let again = if job {
            "run the job again"
        } else {
            "deploy again"
        };
        match self {
            Self::Mount { target, volume } => {
                mount_words(target.as_deref(), volume.as_deref(), again)
            }
            Self::Image { image, pull } => image_words(engine, image.as_deref(), *pull, again),
            Self::Network(Some(name)) => format!(
                "the network '{name}' does not exist on this host. If the Compose file marks it `external: true`, create it first with `{engine} network create {name}`; the networks mm manages are made when it deploys. Then {again}"
            ),
            Self::Network(None) => format!(
                "a network it joins does not exist on this host. If the Compose file marks it `external: true`, create it first with `{engine} network create NAME`; the networks mm manages are made when it deploys. Then {again}"
            ),
            Self::Port(port) => format!(
                "{} is already in use on this host by another container or program. Stop what holds it (`{engine} ps` lists the containers that publish ports), or publish another host port in the service's `ports:`, then {again}",
                port.map_or_else(
                    || "a port it publishes".to_owned(),
                    |port| format!("port {port}")
                )
            ),
            Self::Name(name) => {
                let name = name.as_deref().unwrap_or("NAME");
                let rename = if job {
                    ""
                } else {
                    ", or change `container_name` of the service"
                };
                format!(
                    "another container is already named '{name}'. Remove it with `{engine} rm --force {name}` if nothing needs it{rename}, then {again}"
                )
            }
        }
    }
}

fn mount_words(target: Option<&str>, volume: Option<&str>, again: &str) -> String {
    match (target, volume) {
        (Some(target), Some(volume)) => format!(
            "the volume '{volume}' is a directory, and {target} in the container is a file, so the engine cannot mount the volume there. Mount it at a directory in the service's `volumes:` instead, then {again}"
        ),
        (Some(target), None) => format!(
            "its mount at {target} puts a directory onto a file or a file onto a directory. A bind of a host path that does not exist makes a directory there, so check that the source exists and is the same kind as {target} in the container; change the service's `volumes:` if it is not, then {again}"
        ),
        (None, _) => format!(
            "one of its mounts puts a directory onto a file or a file onto a directory. A volume is always a directory, and a bind of a host path that does not exist makes one; make the source and the target of each mount in the service's `volumes:` the same kind, then {again}"
        ),
    }
}

fn image_words(engine: &str, image: Option<&str>, pull: Pull, again: &str) -> String {
    let named = image.map_or_else(
        || "its image".to_owned(),
        |image| format!("its image {image}"),
    );
    if image.is_some_and(|image| {
        image
            .strip_prefix(LOCAL_IMAGE_REGISTRY)
            .is_some_and(|rest| rest.starts_with('/'))
    }) {
        return format!(
            "{named} is not in the engine; mm builds it on this machine. Build it with `mm dev up`, which builds what is missing, or with `mm build plan` and `mm build apply`, then {again}"
        );
    }
    let reference = image.unwrap_or("IMAGE");
    match pull {
        Pull::Absent => format!(
            "{named} is not on this host, and the engine was not to pull it. Pull it with `{engine} pull {reference}`, then {again}"
        ),
        Pull::Denied => format!(
            "{named} could not be pulled: its registry refused it, because the repository does not exist or this host has to log in first. Check the name in `image:`, and for a private registry log in with `{engine} login {}`, then {again}",
            image.map_or("REGISTRY", registry)
        ),
        Pull::Unknown => format!(
            "{named} could not be pulled: its registry has no such image or tag. Check the name and tag in `image:`, then {again}"
        ),
        Pull::Unreachable => format!(
            "{named} could not be pulled: its registry could not be reached from this host. Check the registry's name in `image:` and this host's network, then {again}"
        ),
        Pull::Limited => format!(
            "{named} could not be pulled: its registry limits how many images this host may pull. Log in with `{engine} login {}`, or wait, then {again}",
            image.map_or("REGISTRY", registry)
        ),
        Pull::ShortName => format!(
            "{named} names no registry, and Podman has none to search. Write the full name in `image:`, such as docker.io/library/{}, then {again}",
            image.unwrap_or("NAME")
        ),
    }
}

fn subject(launched: &Launched<'_>, stderr: &str) -> String {
    match *launched {
        Launched::Job { job, service } => format!("job '{job}' (service '{service}')"),
        Launched::Services {
            stack,
            project,
            services,
        } => match failing(stderr, project).or_else(|| match services {
            [only] => Some(Failing::Service(only.clone())),
            _ => None,
        }) {
            Some(Failing::Service(service)) => format!("service '{service}' of stack '{stack}'"),
            Some(Failing::Container(container)) => {
                format!("container '{container}' of stack '{stack}'")
            }
            None => format!("a service of stack '{stack}'"),
        },
    }
}

enum Failing {
    Service(String),
    Container(String),
}

/// The service or container that Compose names as the one that failed.
fn failing(stderr: &str, project: &str) -> Option<Failing> {
    let named = stderr.lines().find_map(|line| {
        let (service, _) = line
            .trim_start()
            .strip_prefix("service:")?
            .split_once(':')?;
        plain(service).then(|| service.to_owned())
    });
    if let Some(service) = named {
        return Some(Failing::Service(service));
    }
    let container = between(stderr, "on endpoint ", ' ')
        .filter(|name| plain(name))
        .map(str::to_owned)
        .or_else(|| unfinished(stderr))?;
    let service = container
        .strip_prefix(project)
        .and_then(|rest| rest.strip_prefix('-'))
        .and_then(|rest| rest.rsplit_once('-'))
        .filter(|(service, index)| {
            !service.is_empty()
                && !index.is_empty()
                && index.chars().all(|character| character.is_ascii_digit())
        })
        .map(|(service, _)| service.to_owned());
    Some(service.map_or(Failing::Container(container), Failing::Service))
}

/// The one container that Compose began to create or start and did not
/// finish.
fn unfinished(stderr: &str) -> Option<String> {
    let mut last = BTreeMap::new();
    for line in stderr.lines() {
        let mut words = line.split_whitespace();
        if words.next() != Some("Container") {
            continue;
        }
        if let (Some(name), Some(state)) = (words.next(), words.next()) {
            last.insert(name, state);
        }
    }
    let mut open = last
        .into_iter()
        .filter(|(_, state)| matches!(*state, "Creating" | "Starting" | "Recreate" | "Error"))
        .map(|(name, _)| name);
    match (open.next(), open.next()) {
        (Some(name), None) if plain(name) => Some(name.to_owned()),
        _ => None,
    }
}

fn mount_target(stderr: &str) -> Option<String> {
    between(stderr, "to rootfs at \"", '"')
        .or_else(|| {
            between(stderr, "crun: mount `", '\n').and_then(|rest| between(rest, "` to `", '`'))
        })
        .map(|target| {
            if target.starts_with('/') {
                target.to_owned()
            } else {
                format!("/{target}")
            }
        })
        .filter(|target| path_like(target))
}

fn mount_volume(stderr: &str) -> Option<String> {
    let source = between(stderr, "error mounting \"", '"')
        .or_else(|| between(stderr, "crun: mount `", '`'))?;
    let (_, name) = source.strip_suffix("/_data")?.rsplit_once("/volumes/")?;
    plain(name).then(|| name.to_owned())
}

/// `Some` when a network the container joins does not exist, with its
/// name when the engine said it.
fn missing_network(stderr: &str) -> Option<Option<String>> {
    let lower = stderr.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find("network ") {
        let start = from + found + "network ".len();
        from = start;
        let Some(name) = stderr[start..].split_whitespace().next() else {
            break;
        };
        let after = &lower[start..].trim_start()[name.len()..];
        if after.starts_with(" not found") || after.starts_with(" declared as external") {
            return Some(plain(name).then(|| name.to_owned()));
        }
    }
    lower.contains("network not found").then(|| {
        between(stderr, "name or id ", ':')
            .filter(|name| plain(name))
            .map(str::to_owned)
    })
}

fn port(stderr: &str) -> Option<u16> {
    let lower = stderr.to_ascii_lowercase();
    let end = ["port is already allocated", "address already in use"]
        .iter()
        .filter_map(|phrase| lower.find(phrase))
        .min()?;
    let start = lower[..end].rfind('\n').map_or(0, |newline| newline + 1);
    let before = &stderr[start..end];
    before.rmatch_indices([':', '/']).find_map(|(at, _)| {
        let digits = before[at + 1..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        match digits.parse::<u16>() {
            Ok(port) if port > 0 => Some(port),
            _ => None,
        }
    })
}

fn pull_failure(lower: &str) -> Option<Pull> {
    let pulling = lower.contains("failed to resolve reference")
        || lower.contains("/manifests/")
        || lower.contains("initializing source")
        || lower.contains("error pulling image")
        || lower.contains("pinging container registry")
        || lower.lines().any(|line| {
            let mut words = line.split_whitespace();
            words.next() == Some("image")
                && words
                    .nth(1)
                    .is_some_and(|state| state == "pulling" || state == "error")
        });
    Some(
        if lower.contains("no such image") || lower.contains("image not known") {
            Pull::Absent
        } else if lower.contains("toomanyrequests") {
            Pull::Limited
        } else if lower.contains("short-name") && lower.contains("did not resolve") {
            Pull::ShortName
        } else if lower.contains("pull access denied")
            || lower.contains("requested access to the resource is denied")
            || (pulling
                && (lower.contains("unauthorized") || lower.contains("authentication required")))
        {
            Pull::Denied
        } else if lower.contains("manifest unknown")
            || (lower.contains("manifest for ") && lower.contains(" not found"))
            || (lower.contains("failed to resolve reference") && lower.contains(": not found"))
        {
            Pull::Unknown
        } else if pulling
            && [
                "no such host",
                "dial tcp",
                "i/o timeout",
                "connection refused",
                "network is unreachable",
                "tls: ",
                "x509: ",
            ]
            .iter()
            .any(|known| lower.contains(known))
        {
            Pull::Unreachable
        } else {
            return None;
        },
    )
}

fn image(stderr: &str) -> Option<String> {
    let progress = stderr.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        if words.next() == Some("Image") {
            words.next()
        } else {
            None
        }
    });
    let token = |marker: &str| {
        between(stderr, marker, '\n')
            .and_then(|rest| rest.split_whitespace().next())
            .map(|token| token.trim_end_matches([':', ',']))
    };
    [
        progress,
        token("no such image: "),
        between(stderr, "pull access denied for ", ','),
        token("manifest for "),
        between(stderr, "failed to resolve reference \"", '"'),
        token("initializing source docker://"),
        between(stderr, "short-name \"", '"'),
    ]
    .into_iter()
    .flatten()
    .find(|image| image_like(image))
    .map(str::to_owned)
}

/// The registry an image is pulled from.
fn registry(image: &str) -> &str {
    match image.split_once('/') {
        Some((first, _)) if first.contains(['.', ':']) || first == "localhost" => first,
        _ => "docker.io",
    }
}

/// The text after the first `marker`, matched without regard to case, up
/// to `end` or the end of `text`.
fn between<'a>(text: &'a str, marker: &str, end: char) -> Option<&'a str> {
    let start = text
        .to_ascii_lowercase()
        .find(&marker.to_ascii_lowercase())?
        + marker.len();
    let rest = &text[start..];
    Some(rest.find(end).map_or(rest, |stop| &rest[..stop]))
}

fn plain(name: &str) -> bool {
    name.len() <= 128
        && name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
}

fn path_like(path: &str) -> bool {
    path.len() <= 512
        && path.len() > 1
        && path
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "/-_.@+".contains(character))
}

fn image_like(image: &str) -> bool {
    image.len() <= 255
        && image
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && !image.contains("//")
        && image
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./:@".contains(character))
}
