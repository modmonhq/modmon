//! The hash of what an environment's workloads are meant to be: its images
//! and the networks its services share.

use std::collections::BTreeMap;

use crate::domain::workspace::ProjectId;

pub(in crate::app::stacks) fn canonical_workload_intent_hash(
    project_id: ProjectId,
    environment: &str,
    images: &BTreeMap<String, String>,
    networks: &[crate::domain::stack::CanonicalNetworkIntent],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hash_intent(&mut hasher, "mm-workload-intent-v1");
    hash_intent(&mut hasher, &project_id.0.to_string());
    hash_intent(&mut hasher, environment);
    hasher.update(&(images.len() as u64).to_le_bytes());
    for (module, image) in images {
        hash_intent(&mut hasher, module);
        hash_intent(&mut hasher, image);
    }
    hasher.update(&(networks.len() as u64).to_le_bytes());
    for network in networks {
        hash_intent(&mut hasher, &network.logical_name);
        hash_intent(&mut hasher, &network.physical_name);
        hasher.update(&(network.members.len() as u64).to_le_bytes());
        for member in &network.members {
            hash_intent(&mut hasher, &member.stack);
            hash_intent(&mut hasher, &member.service);
        }
    }
    hasher.finalize().to_hex().to_string()
}

fn hash_intent(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}
