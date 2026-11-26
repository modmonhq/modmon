//! Stacks that roll out side by side.
//!
//! The stages of a cluster rollout go in waves, one per layer of the
//! stacks' `after:` order. A stage shares its wave with the other stages
//! of its layer when it has no jobs, no `ahead` services, no service that
//! pauses on failure, and one batch of nodes; any other stage is a wave of
//! its own, in the stages' order.

use std::collections::{BTreeMap, BTreeSet};

use super::RolloutStage;

/// How many stacks a node rolls out at once in a shared wave.
pub const STACKS_AT_ONCE: usize = 4;

/// Whether `stage` may roll out side by side with others.
#[must_use]
pub fn shares_a_wave(stage: &RolloutStage) -> bool {
    stage.jobs.is_empty()
        && stage.ahead.is_empty()
        && stage.pause_on_failure.is_empty()
        && stage.batches.len() == 1
}

/// The layer of each stage: 0 for a stage that follows none of the others,
/// else one more than the deepest stage it follows. `after` maps a stack to
/// the stacks it follows.
fn layers(stages: &[RolloutStage], after: &BTreeMap<String, BTreeSet<String>>) -> Vec<usize> {
    let mut layers = vec![0; stages.len()];
    for (index, stage) in stages.iter().enumerate() {
        let follows = after.get(&stage.stack);
        let layer = stages[..index]
            .iter()
            .enumerate()
            .filter(|(_, earlier)| follows.is_some_and(|follows| follows.contains(&earlier.stack)))
            .map(|(earlier, _)| layers[earlier] + 1)
            .max()
            .unwrap_or(0);
        layers[index] = layer;
    }
    layers
}

/// The waves of `stages`, as indices into them. Empty when no two stages
/// roll out side by side.
#[must_use]
pub fn waves_of(
    stages: &[RolloutStage],
    after: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<Vec<usize>> {
    let layers = layers(stages, after);
    let mut order = (0..stages.len()).collect::<Vec<_>>();
    order.sort_by_key(|index| (layers[*index], *index));
    let mut waves = Vec::<Vec<usize>>::new();
    let mut shared = Vec::<usize>::new();
    let mut layer = 0;
    for index in order {
        if layers[index] != layer && !shared.is_empty() {
            waves.push(std::mem::take(&mut shared));
        }
        layer = layers[index];
        if shares_a_wave(&stages[index]) {
            shared.push(index);
        } else {
            if !shared.is_empty() {
                waves.push(std::mem::take(&mut shared));
            }
            waves.push(vec![index]);
        }
    }
    if !shared.is_empty() {
        waves.push(shared);
    }
    if waves.iter().all(|wave| wave.len() < 2) {
        return Vec::new();
    }
    waves
}

/// Checks `waves` against `stages` and this node's `after:` order: every
/// stage once, a shared wave of stages that may share one, and every stage
/// after the stages it follows.
pub fn check_waves(
    waves: &[Vec<usize>],
    stages: &[RolloutStage],
    after: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), String> {
    if waves.is_empty() {
        return Ok(());
    }
    let mut wave_of = vec![None; stages.len()];
    for (number, wave) in waves.iter().enumerate() {
        for index in wave {
            let slot = wave_of
                .get_mut(*index)
                .ok_or("the cluster plan's waves name a stage it does not have")?;
            if slot.replace(number).is_some() {
                return Err("the cluster plan's waves name a stage twice".to_owned());
            }
            if wave.len() > 1 && !shares_a_wave(&stages[*index]) {
                return Err(format!(
                    "the cluster plan rolls stack '{}' out beside others, which its jobs or settings do not allow",
                    stages[*index].stack
                ));
            }
        }
    }
    for (index, stage) in stages.iter().enumerate() {
        let Some(wave) = wave_of[index] else {
            return Err(format!(
                "the cluster plan's waves leave out stack '{}'",
                stage.stack
            ));
        };
        for earlier in after.get(&stage.stack).into_iter().flatten() {
            let before = stages
                .iter()
                .position(|other| &other.stack == earlier)
                .and_then(|position| wave_of[position]);
            if before.is_some_and(|before| before >= wave) {
                return Err(format!(
                    "the cluster plan rolls stack '{}' out before or beside '{earlier}', which `after:` puts first",
                    stage.stack
                ));
            }
        }
    }
    Ok(())
}
