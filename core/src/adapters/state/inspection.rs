use std::path::PathBuf;
use std::sync::Arc;

use crate::app::ports::{StateInspector, StateStore};
use crate::domain::error::MmError;

use super::SqliteStateStore;

pub struct SqliteIngressRouteReader {
    pub path: PathBuf,
}

impl crate::app::ports::IngressRouteReader for SqliteIngressRouteReader {
    fn read_routes(
        &self,
        target_id: crate::domain::target::TargetId,
    ) -> Result<crate::domain::ingress::IngressRouteSet, MmError> {
        use crate::app::ports::IngressRouteRegistry;
        match SqliteStateStore::inspect(&self.path)? {
            Some(state) => state.route_set(target_id),
            None => Ok(crate::domain::ingress::IngressRouteSet {
                target_id,
                generation: 0,
                config_digest: None,
                claims: Vec::new(),
                middlewares: Default::default(),
            }),
        }
    }
}

pub struct SqliteStateInspector {
    pub path: PathBuf,
}

impl StateInspector for SqliteStateInspector {
    fn inspect(&self) -> Result<Option<Arc<dyn StateStore>>, MmError> {
        Ok(SqliteStateStore::inspect(&self.path)?
            .map(|state| Arc::new(state) as Arc<dyn StateStore>))
    }
}
