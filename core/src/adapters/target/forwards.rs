//! Saved forwards (`mm forward save`), private to this machine under MM_HOME.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::domain::error::MmError;
use crate::domain::forward::{ForwardShortcut, ForwardShortcuts};

#[derive(Debug, Clone)]
pub struct ForwardShortcutStore {
    path: PathBuf,
}

impl ForwardShortcutStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load_all(&self) -> Result<ForwardShortcuts, MmError> {
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

    fn store_all(&self, all: &ForwardShortcuts) -> Result<(), MmError> {
        let mut text = serde_json::to_string_pretty(all).map_err(|error| MmError::State {
            message: format!("serializing saved forwards: {error}"),
        })?;
        text.push('\n');
        let parent = self.path.parent().ok_or_else(|| MmError::State {
            message: "saved forwards file has no directory".to_owned(),
        })?;
        std::fs::create_dir_all(parent).map_err(|source| MmError::Io {
            context: format!("creating {}", parent.display()),
            source,
        })?;
        let temporary = parent.join(format!(".forwards.{}", uuid::Uuid::new_v4()));
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
                tracing::warn!(%cleanup, "could not remove a temporary saved-forwards file");
            }
            MmError::Io {
                context: format!("writing {}", self.path.display()),
                source,
            }
        })
    }

    pub fn list(&self, project: &str) -> Result<BTreeMap<String, ForwardShortcut>, MmError> {
        Ok(self.load_all()?.remove(project).unwrap_or_default())
    }

    pub fn save(
        &self,
        project: &str,
        name: &str,
        shortcut: ForwardShortcut,
    ) -> Result<(), MmError> {
        let mut all = self.load_all()?;
        all.entry(project.to_owned())
            .or_default()
            .insert(name.to_owned(), shortcut);
        self.store_all(&all)
    }

    pub fn remove(&self, project: &str, name: &str) -> Result<bool, MmError> {
        let mut all = self.load_all()?;
        let removed = all
            .get_mut(project)
            .is_some_and(|shortcuts| shortcuts.remove(name).is_some());
        if removed {
            self.store_all(&all)?;
        }
        Ok(removed)
    }
}
