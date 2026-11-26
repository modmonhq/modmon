//! The containers mm started on this node, from the engine's API.

use crate::adapters::engine_api::EngineApi;
use crate::adapters::runtime::engine_containers::inspected_containers;
use crate::app::ports::NodeContainers;
use crate::domain::error::MmError;
use crate::domain::node_metrics::{LabelledContainer, MAX_CONTAINERS};

#[derive(Debug, Clone)]
pub struct EngineServiceContainers {
    api: Option<EngineApi>,
}

impl EngineServiceContainers {
    /// Reads `api`, the engine setup installed; `None` when `mmd` has no
    /// socket of it to read.
    #[must_use]
    pub fn new(api: Option<EngineApi>) -> Self {
        Self { api }
    }
}

#[async_trait::async_trait]
impl NodeContainers for EngineServiceContainers {
    async fn containers(&self) -> Result<Vec<LabelledContainer>, MmError> {
        let api = self.api.as_ref().ok_or_else(|| MmError::Environment {
            message: "the node's engine has no Unix socket this daemon can read: DOCKER_HOST or the Docker context of mmd's environment names another engine; remove it from mmd's environment".to_owned(),
        })?;
        let filters = serde_json::json!({ "label": [crate::domain::ownership::LABEL_PROJECT_ID] });
        Ok(inspected_containers(api, &filters)
            .await?
            .iter()
            .filter_map(LabelledContainer::from_inspected)
            .take(MAX_CONTAINERS)
            .collect())
    }
}
