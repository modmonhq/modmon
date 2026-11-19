//! The unknown keys of the configuration a command read, each printed once
//! on stderr.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::domain::config::UnknownConfigKey;
use crate::domain::remote::ClusterDeploymentPlan;

static PRINTED: Mutex<BTreeSet<(String, String)>> = Mutex::new(BTreeSet::new());
static PENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn lock<T>(mutex: &'static Mutex<T>) -> MutexGuard<'static, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Prints the unknown keys of every configuration this command reads.
pub fn listen() {
    crate::app::unknown_keys::listen(|keys| warn(keys, "this mm"));
}

/// Prints each of `keys` the command has not printed yet, as `reader`
/// ignores it.
pub fn warn(keys: &[UnknownConfigKey], reader: &str) {
    {
        let mut printed = lock(&PRINTED);
        let mut pending = lock(&PENDING);
        for key in keys {
            if printed.insert((key.file.clone(), key.key.clone())) {
                pending.push(format!("Warning: {}", key.message(reader)));
            }
        }
    }
    flush();
}

/// Prints the unknown keys the hosts that planned `plan` ignore, with the
/// nodes that ignore each.
pub fn warn_hosts(plan: &ClusterDeploymentPlan) {
    let mut readers = BTreeMap::<&UnknownConfigKey, Vec<&str>>::new();
    for node in &plan.nodes {
        for key in node
            .plan
            .plan
            .iter()
            .flat_map(|planned| &planned.unknown_config_keys)
        {
            let names = readers.entry(key).or_default();
            names.extend(node.node.as_deref());
        }
    }
    for (key, nodes) in readers {
        let reader = if nodes.is_empty() {
            "the host's mmd".to_owned()
        } else {
            format!("mmd on {}", nodes.join(", "))
        };
        warn(std::slice::from_ref(key), &reader);
    }
}

/// Prints the warnings held back while a live view was drawn.
pub fn flush() {
    if crate::presentation::inline::painting() {
        return;
    }
    let lines = std::mem::take(&mut *lock(&PENDING));
    for line in lines {
        crate::presentation::record::line(&line);
        eprintln!("{}", crate::presentation::output::terminal_safe(&line));
    }
}
