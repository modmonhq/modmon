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
        super::private_json::read(&self.path)
    }

    fn store_all(&self, all: &ForwardShortcuts) -> Result<(), MmError> {
        super::private_json::write(&self.path, all, "saved forwards")
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
