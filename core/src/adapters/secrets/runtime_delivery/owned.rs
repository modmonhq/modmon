// The copies of a file secret: on rootful Podman one for each consumer,
// given to the host ids its `uid` and `gid` are in its user namespace; on
// Docker Engine one for each owner and mode its consumers ask for.

/// Who a consumer's copy of a revision belongs to on the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct FileOwner {
    uid: u32,
    gid: u32,
    mode: u32,
}

impl FileOwner {
    /// The name of the copy of revision file `revision` for this owner.
    fn file_name(owner: Option<Self>, revision: &str) -> String {
        owner.map_or_else(
            || revision.to_owned(),
            |owner| format!("{revision}-{}.{}.{:o}", owner.uid, owner.gid, owner.mode),
        )
    }

    /// The revision file a file name is a copy of, and the owner part.
    fn split(name: &str) -> (&str, Option<&str>) {
        match name.rsplit_once('-') {
            Some((revision, owner)) if owner.contains('.') => (revision, Some(owner)),
            _ => (name, None),
        }
    }

    /// The host owner of `consumer`'s copy: its ids as the container's
    /// user namespace has them on the host.
    fn of(plan: &RuntimeSecretPlan, consumer: &RuntimeSecretConsumer) -> Result<Self, MmError> {
        let first = plan.first_host_id(consumer);
        let host = |value: &str, name: &str| {
            match (value.parse::<u32>(), first) {
                (Ok(id), None) => Some(id),
                (Ok(id), Some(first)) if id < crate::domain::user_namespace::SERVICE_IDS => {
                    first.checked_add(id)
                }
                (Ok(_) | Err(_), _) => None,
            }
            .ok_or_else(|| MmError::Secret {
                message: format!(
                    "secret file {} of service '{}': {name} {value} is none of the ids its containers have, 0 to {}",
                    consumer.target,
                    consumer.service,
                    crate::domain::user_namespace::SERVICE_IDS - 1
                ),
            })
        };
        Ok(Self {
            uid: host(&consumer.uid, "uid")?,
            gid: host(&consumer.gid, "gid")?,
            mode: consumer.mode,
        })
    }

    /// The owner of `consumer`'s copy on Docker Engine, whose containers
    /// have the host's ids: `user` when mm runs as that user, else the ids
    /// the consumer names. `None` for the copy that belongs to the user mm
    /// runs as with mode 0444, the revision's own file.
    fn on_docker(
        consumer: &RuntimeSecretConsumer,
        user: Option<HostIds>,
    ) -> Result<Option<Self>, MmError> {
        let id = |value: &str, name: &str| {
            value.parse::<u32>().map_err(|_| MmError::Secret {
                message: format!(
                    "secret file {} of service '{}': {name} {value} is no id of this host; name one from 0 to {}",
                    consumer.target,
                    consumer.service,
                    u32::MAX
                ),
            })
        };
        let owner = match user {
            Some(user) => Self {
                uid: user.uid,
                gid: user.gid,
                mode: consumer.mode,
            },
            None => Self {
                uid: id(&consumer.uid, "uid")?,
                gid: id(&consumer.gid, "gid")?,
                mode: consumer.mode,
            },
        };
        let me = user.unwrap_or(HostIds { uid: 0, gid: 0 });
        let own = Self {
            uid: me.uid,
            gid: me.gid,
            mode: 0o444,
        };
        Ok((owner != own).then_some(owner))
    }
}

/// Gives a staged copy to its owner, with its mode.
fn give_to(path: &Path, owner: FileOwner) -> Result<(), MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    fs::metadata(path)
        .and_then(|metadata| {
            if (metadata.uid(), metadata.gid()) == (owner.uid, owner.gid) {
                Ok(())
            } else {
                std::os::unix::fs::chown(path, Some(owner.uid), Some(owner.gid))
            }
        })
        .and_then(|()| fs::set_permissions(path, fs::Permissions::from_mode(owner.mode)))
        .map_err(|source| MmError::Io {
            context: "giving a runtime secret file to the user of its container".to_owned(),
            source,
        })
}

/// Lets every user of a container that mounts the file read it.
fn readable_by_all(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o444)).map_err(|source| MmError::Io {
        context: "letting the users of a container read a runtime secret file".to_owned(),
        source,
    })
}
