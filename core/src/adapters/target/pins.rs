//! Keys this client pinned for repo-declared targets on first use. A commit
//! that changes a target's host key or daemon identity is refused until the
//! developer reviews it with `mm target trust`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::domain::error::MmError;
use crate::domain::repo_target::TargetPin;

#[derive(Debug, Clone)]
pub struct TargetPins {
    path: PathBuf,
}

impl TargetPins {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load(&self) -> Result<BTreeMap<String, TargetPin>, MmError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| MmError::ConfigParse {
                path: self.path.clone(),
                message: error.to_string(),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(source) => Err(MmError::Io {
                context: format!("reading {}", self.path.display()),
                source,
            }),
        }
    }

    fn store(&self, pins: &BTreeMap<String, TargetPin>) -> Result<(), MmError> {
        let mut text = serde_json::to_string_pretty(pins).map_err(|error| MmError::State {
            message: format!("serializing target pins: {error}"),
        })?;
        text.push('\n');
        let parent = self.path.parent().ok_or_else(|| MmError::State {
            message: "target pins file has no directory".to_owned(),
        })?;
        std::fs::create_dir_all(parent).map_err(|source| MmError::Io {
            context: format!("creating {}", parent.display()),
            source,
        })?;
        let temporary = parent.join(format!(".target-pins.{}", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let written = options
            .open(&temporary)
            .and_then(|mut file| {
                std::io::Write::write_all(&mut file, text.as_bytes())?;
                file.sync_all()
            })
            .and_then(|()| std::fs::rename(&temporary, &self.path));
        written.map_err(|source| {
            if let Err(cleanup) = std::fs::remove_file(&temporary)
                && cleanup.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%cleanup, "could not remove a temporary target pins file");
            }
            MmError::Io {
                context: format!("writing {}", self.path.display()),
                source,
            }
        })
    }

    #[must_use]
    pub fn pinned(&self, key: &str) -> Option<TargetPin> {
        self.load().ok().and_then(|pins| pins.get(key).cloned())
    }

    /// Records `pin` the first time; afterwards refuses any host key or
    /// daemon identity it has not seen. Dropping or reordering nodes only
    /// narrows the pin, so a dropped node needs review to come back.
    pub fn check_or_record(&self, key: &str, name: &str, pin: &TargetPin) -> Result<(), MmError> {
        let mut pins = self.load()?;
        match pins.get(key) {
            Some(existing) if existing == pin => Ok(()),
            Some(existing) if existing.covers(pin) => {
                pins.insert(key.to_owned(), pin.clone());
                self.store(&pins)
            }
            Some(_) => Err(MmError::Security {
                message: format!(
                    "the repo changed a host key or daemon identity of target '{name}' since you first connected; review it with `mm target trust {name}`"
                ),
            }),
            None => {
                pins.insert(key.to_owned(), pin.clone());
                self.store(&pins)
            }
        }
    }

    /// Accepts the repo's current facts after review.
    pub fn trust(&self, key: &str, pin: &TargetPin) -> Result<(), MmError> {
        let mut pins = self.load()?;
        pins.insert(key.to_owned(), pin.clone());
        self.store(&pins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::repo_target::NodePin;

    /// A cluster pin: one node per letter, named `node-<letter>`.
    fn pin(nodes: &[&str]) -> TargetPin {
        let mut pin = TargetPin::default();
        pin.nodes = nodes
            .iter()
            .map(|node| NodePin {
                name: format!("node-{node}"),
                host_key: format!("ssh-ed25519 {node}"),
                daemon_fingerprint: node.repeat(64),
            })
            .collect();
        pin
    }

    fn host(letter: &str) -> TargetPin {
        let mut pin = TargetPin::default();
        pin.host_key = Some(format!("ssh-ed25519 {letter}"));
        pin.daemon_fingerprint = Some(letter.repeat(64));
        pin
    }

    #[test]
    fn new_node_keys_need_review_but_dropping_nodes_does_not()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = crate::test_support::tempdir()?;
        let pins = TargetPins::new(temporary.path().join("pinned-targets.json"));
        pins.check_or_record("p/prod", "prod", &pin(&["a", "b", "c"]))?;

        let added = pins.check_or_record("p/prod", "prod", &pin(&["a", "b", "c", "d"]));
        assert!(
            matches!(&added, Err(MmError::Security { message }) if message.contains("mm target trust prod")),
            "a new node is refused: {added:?}"
        );
        let swapped = pins.check_or_record("p/prod", "prod", &pin(&["a", "c", "b"]));
        assert!(swapped.is_ok(), "reordering adds no identity: {swapped:?}");
        let fewer = pins.check_or_record("p/prod", "prod", &pin(&["b", "c"]));
        assert!(fewer.is_ok(), "dropping a node adds no identity: {fewer:?}");
        assert_eq!(pins.pinned("p/prod"), Some(pin(&["b", "c"])));

        let returned = pins.check_or_record("p/prod", "prod", &pin(&["b", "c", "a"]));
        assert!(returned.is_err(), "a dropped node needs review to return");
        let mut mixed = pin(&["b"]);
        mixed.nodes[0].daemon_fingerprint = "c".repeat(64);
        assert!(
            pins.check_or_record("p/prod", "prod", &mixed).is_err(),
            "a known host key with another node's daemon is a new identity"
        );

        pins.trust("p/prod", &pin(&["b", "c", "a"]))?;
        pins.check_or_record("p/prod", "prod", &pin(&["b", "c", "a"]))?;
        let stored = std::fs::read_to_string(temporary.path().join("pinned-targets.json"))?;
        assert!(
            stored.contains(r#""name": "node-a""#),
            "pins name their nodes: {stored}"
        );
        Ok(())
    }

    #[test]
    fn single_host_pins_keep_their_format() -> Result<(), Box<dyn std::error::Error>> {
        let temporary = crate::test_support::tempdir()?;
        let path = temporary.path().join("pinned-targets.json");
        std::fs::write(
            &path,
            format!(
                r#"{{"p/prod": {{"host-key": "ssh-ed25519 a", "daemon-fingerprint": "{}"}}}}"#,
                "a".repeat(64)
            ),
        )?;
        let pins = TargetPins::new(path.clone());
        assert_eq!(pins.pinned("p/prod"), Some(host("a")));
        pins.check_or_record("p/prod", "prod", &host("a"))?;
        assert!(
            pins.check_or_record("p/prod", "prod", &host("b")).is_err(),
            "another host key needs review"
        );
        // A host that becomes the first node of a cluster keeps its keys:
        // only the nodes added beside it need review.
        pins.check_or_record("p/prod", "prod", &pin(&["a"]))?;
        assert!(
            pins.check_or_record("p/prod", "prod", &pin(&["a", "b"]))
                .is_err()
        );
        // A pin from a build that recorded the other hosts without names
        // still covers them once the repo names them.
        std::fs::write(
            &path,
            format!(
                r#"{{"p/old": {{"host-key": "ssh-ed25519 a", "daemon-fingerprint": "{}", "members": [{{"host-key": "ssh-ed25519 b", "daemon-fingerprint": "{}"}}]}}}}"#,
                "a".repeat(64),
                "b".repeat(64)
            ),
        )?;
        pins.check_or_record("p/old", "old", &pin(&["a", "b"]))?;
        assert_eq!(pins.pinned("p/old"), Some(pin(&["a", "b"])));
        assert!(
            pins.check_or_record("p/old", "old", &pin(&["a", "b", "c"]))
                .is_err()
        );
        pins.trust("p/other", &host("z"))?;
        let stored = std::fs::read_to_string(&path)?;
        assert!(
            stored.contains(
                r#""p/other": {
    "host-key": "ssh-ed25519 z""#
            ),
            "{stored}"
        );
        Ok(())
    }
}
