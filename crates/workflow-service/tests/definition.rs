use serde_json::json;
use workflow_service::definition::validate;

#[test]
fn validation_reports_only_supported_schema_versions() {
    let graph = |version| json!({"schemaVersion":version,"id":"v","name":"Version","nodes":[{"id":"start","type":"Manual Trigger","label":"Start"}],"edges":[]});
    for version in [1, 2] {
        let result = validate(&graph(json!(version)));
        assert_eq!(result["valid"], true, "{result}");
        assert_eq!(result["schemaVersion"], version);
    }
    for version in [json!(99), json!("abc"), json!({"nested":true}), json!(-1)] {
        let result = validate(&graph(version.clone()));
        assert_eq!(result["valid"], false, "{version}");
        assert_eq!(
            result["schemaVersion"],
            workflow_schema::LATEST_SCHEMA_VERSION,
            "{version}"
        );
        assert!(!result.to_string().contains("nested"), "{result}");
    }
    let missing = validate(&json!({"id":"v"}));
    assert_eq!(
        missing["schemaVersion"],
        workflow_schema::LATEST_SCHEMA_VERSION
    );
}
