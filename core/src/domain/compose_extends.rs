//! Compose `extends` within one file.

use serde_yaml_ng::{Mapping, Value};

/// How many services one `extends` chain may pass through.
const EXTENDS_DEPTH: usize = 16;

/// Each service of the file as Compose runs it: a service that `extends`
/// another of the same file gets what it inherits merged in.
pub fn resolve_extends(compose: &mut Mapping) {
    let Some(services) = compose.get("services").and_then(Value::as_mapping) else {
        return;
    };
    let resolved = services
        .keys()
        .map(|name| (name.clone(), extended(services, name, 0)))
        .collect::<Vec<_>>();
    if let Some(services) = compose.get_mut("services").and_then(Value::as_mapping_mut) {
        for (name, service) in resolved {
            services.insert(name, service);
        }
    }
}

/// Service `name` with the service it extends merged under it, as deep as
/// [`EXTENDS_DEPTH`]: a loop or a missing service stops there, as Compose
/// refuses both.
fn extended(services: &Mapping, name: &Value, depth: usize) -> Value {
    let Some(service) = services.get(name) else {
        return Value::Null;
    };
    let base = match service.get("extends") {
        Some(Value::String(base)) => Some(base.as_str()),
        Some(Value::Mapping(extends)) if extends.get("file").is_none() => {
            extends.get("service").and_then(Value::as_str)
        }
        _ => None,
    };
    let Some(base) = base.filter(|_| depth < EXTENDS_DEPTH) else {
        return service.clone();
    };
    let mut merged = extended(services, &Value::String(base.to_owned()), depth + 1);
    let (Some(merged_mapping), Some(own)) = (merged.as_mapping_mut(), service.as_mapping()) else {
        return service.clone();
    };
    merged_mapping.remove("extends");
    for (key, value) in own
        .iter()
        .filter(|(key, _)| key.as_str() != Some("extends"))
    {
        match merged_mapping.get_mut(key) {
            Some(inherited) => merge_extended(inherited, value),
            None => {
                merged_mapping.insert(key.clone(), value.clone());
            }
        }
    }
    merged
}

/// What Compose does with a key both services set: mappings merge, lists
/// add up, and anything else is the extending service's own.
fn merge_extended(inherited: &mut Value, own: &Value) {
    match (inherited, own) {
        (Value::Mapping(inherited), Value::Mapping(own)) => {
            for (key, value) in own {
                match inherited.get_mut(key) {
                    Some(existing) => merge_extended(existing, value),
                    None => {
                        inherited.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (Value::Sequence(inherited), Value::Sequence(own)) => {
            inherited.extend(own.iter().cloned());
        }
        (inherited, own) => *inherited = own.clone(),
    }
}
