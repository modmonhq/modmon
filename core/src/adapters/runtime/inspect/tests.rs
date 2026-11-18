use serde_json::{Value, json};

use super::*;
use crate::domain::ownership::ResourceKind;
use crate::domain::target::TargetId;

fn output(stdout: &str) -> CommandOutput {
    CommandOutput {
        code: Some(0),
        stdout: stdout.to_owned().into(),
        stderr: String::new().into(),
        truncated: false,
    }
}

fn ownership() -> ResourceOwnership {
    ResourceOwnership {
        project_id: None,
        environment: None,
        target_id: TargetId(uuid::Uuid::from_u128(40)),
        kind: ResourceKind::Network,
        logical_name: "demo-network".to_owned(),
        resource: "frontend".to_owned(),
        generation: 1,
    }
}

fn owned_labels(ownership: &ResourceOwnership) -> Result<Value, serde_json::Error> {
    serde_json::to_value(ownership.labels())
}

#[test]
fn reads_all_resource_and_container_label_shapes() {
    for inspected in [
        json!({"Labels": {"generation": "1", "ignored": 1}}),
        json!({"labels": {"generation": "1"}}),
        json!({"Config": {"Labels": {"generation": "1"}}}),
        json!({"config": {"labels": {"generation": "1"}}}),
        json!({"Spec": {"Labels": {"generation": "1"}}}),
        json!({"spec": {"labels": {"generation": "1"}}}),
    ] {
        assert_eq!(
            inspect_labels(&inspected)
                .and_then(|labels| labels.get("generation").cloned())
                .as_deref(),
            Some("1")
        );
    }
    assert_eq!(inspect_labels(&json!({"Labels": "invalid"})), None);
    assert_eq!(inspect_labels(&json!({})), None);
}

#[test]
fn parses_one_owned_resource_and_rejects_untrusted_inspection()
-> Result<(), Box<dyn std::error::Error>> {
    let ownership = ownership();
    for name_key in ["Name", "name"] {
        let row = json!({name_key: ownership.logical_name, "Labels": owned_labels(&ownership)?});
        assert_eq!(
            parse_inspected_resource(output(&json!([row]).to_string()), &ownership, "Docker")?
                [name_key],
            "demo-network"
        );
    }

    let mut truncated = output("[]");
    truncated.truncated = true;
    assert!(parse_inspected_resource(truncated, &ownership, "Docker").is_err());
    for invalid in ["not-json", "[]", "[{},{}]", "[{}]"] {
        assert!(parse_inspected_resource(output(invalid), &ownership, "Docker").is_err());
    }
    let wrong_name = json!([{"Name": "other", "Labels": owned_labels(&ownership)?}]);
    assert!(
        parse_inspected_resource(output(&wrong_name.to_string()), &ownership, "Docker").is_err()
    );
    let missing_labels = json!([{"Name": ownership.logical_name}]);
    assert!(
        parse_inspected_resource(output(&missing_labels.to_string()), &ownership, "Docker")
            .is_err()
    );
    Ok(())
}

#[test]
fn emits_ordered_ownership_label_arguments() {
    let ownership = ownership();
    let args = ownership_label_args(&ownership);
    assert_eq!(args.len(), ownership.labels().len() * 2);
    let (pairs, remainder) = args.as_chunks::<2>();
    assert!(remainder.is_empty());
    assert!(pairs.iter().all(|pair| pair[0] == "--label"));
    let assignments = pairs
        .iter()
        .map(|pair| pair[1].as_str())
        .collect::<Vec<_>>();
    let expected = ownership
        .labels()
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    assert_eq!(assignments, expected);
}

#[test]
fn parses_array_and_ndjson_resource_ids_and_rejects_invalid_rows()
-> Result<(), crate::domain::error::MmError> {
    assert_eq!(
        parse_resource_ids(&output(""), "Docker", "network")?,
        Vec::<String>::new()
    );
    assert_eq!(
        parse_resource_ids(
            &output(r#"[{"ID":"one"},{"Id":"two"},{"id":"three"}]"#),
            "Docker",
            "network"
        )?,
        vec!["one", "two", "three"]
    );
    assert_eq!(
        parse_resource_ids(
            &output("{\"ID\":\"one\"}\n\n{\"id\":\"two\"}\n"),
            "Docker",
            "volume"
        )?,
        vec!["one", "two"]
    );
    for invalid in ["not-json", "[", r#"[{"ID":""}]"#, "[{}]"] {
        assert!(parse_resource_ids(&output(invalid), "Docker", "network").is_err());
    }
    let mut truncated = output("[]");
    truncated.truncated = true;
    assert!(parse_resource_ids(&truncated, "Docker", "network").is_err());
    Ok(())
}

#[test]
fn verifies_project_resource_ownership_and_rejects_invalid_responses()
-> Result<(), Box<dyn std::error::Error>> {
    let ownership = ownership();
    let owned = json!([{"Spec": {"Labels": owned_labels(&ownership)?}}]);
    assert!(
        require_inspected_project_ownership(output(&owned.to_string()), &ownership, "Docker")
            .is_ok()
    );

    let mut truncated = output("[]");
    truncated.truncated = true;
    assert!(require_inspected_project_ownership(truncated, &ownership, "Docker").is_err());
    for invalid in ["not-json", "[]", "[{},{}]", "[{}]"] {
        assert!(
            require_inspected_project_ownership(output(invalid), &ownership, "Docker").is_err()
        );
    }
    Ok(())
}
