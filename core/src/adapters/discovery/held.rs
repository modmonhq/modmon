//! This node's containers past their restart limit (`domain::healing`):
//! answered only while their service has no other container to answer with.

use std::collections::BTreeSet;

use super::{NameResolver, leaving};

impl NameResolver {
    /// From now on, the containers whose engine ids are `held` are past
    /// their restart limit.
    pub fn hold(&self, held: BTreeSet<String>) {
        let mut state = self.write();
        if state.held == held {
            return;
        }
        let before = leaving::answers(&state);
        state.held = held;
        let held = state.held.clone();
        state.local.hold(&held);
        self.settle(&mut state, before);
    }
}
