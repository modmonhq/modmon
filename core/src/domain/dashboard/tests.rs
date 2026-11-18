use super::dashboard_contract_schema;

#[test]
fn dashboard_schema_matches_the_generated_frontend_contract() {
    let checked_in: serde_json::Value =
        serde_json::from_str(include_str!("../../../ui/src/dashboard.schema.json"))
            .unwrap_or_default();
    let generated = serde_json::to_value(dashboard_contract_schema()).unwrap_or_default();
    assert_eq!(checked_in, generated);
}
