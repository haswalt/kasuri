use serde_json::{Value, json};
use workflow_mcp::{auth::Auth, role, rpc};
use workflow_service::Workspace;
const RESOURCE: &str = "https://kasuri.example/mcp/discovery";
fn grant(auth: &mut Auth, profile: &str) -> String {
    let mut index = 0;
    auth.issue(profile, role(profile), RESOURCE, 0, 86400000, &mut || {
        index += 1;
        format!("{profile}-{index:064}")
    })
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string()
}
fn call(ws: &mut Workspace, auth: &mut Auth, token: &str, name: &str, args: Value) -> Value {
    let c = auth.authenticate(token, RESOURCE, 1).unwrap();
    rpc(ws,auth,&c,json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),1,"test-run").unwrap()["result"].clone()
}
fn data(ws: &mut Workspace, auth: &mut Auth, token: &str, name: &str, args: Value) -> Value {
    let result = call(ws, auth, token, name, args);
    assert_eq!(result["isError"], false, "{name}: {result}");
    assert!(result.to_string().len() <= 131072);
    result["structuredContent"].clone()
}
#[test]
fn discovery_and_every_tool_schema_are_valid_offline_and_public() {
    let mut ws = Workspace::default();
    let mut auth = Auth::default();
    let token = grant(&mut auth, "reader");
    let discovery = data(
        &mut ws,
        &mut auth,
        &token,
        "workflows_schema_get",
        json!({}),
    );
    assert_eq!(
        discovery["schema"]["properties"]["schemaVersion"]["enum"],
        json!([1, 2])
    );
    assert_eq!(
        discovery["schema"]["$defs"]["Node"]["properties"]["type"]["type"],
        "string"
    );
    let validator = jsonschema::validator_for(&discovery["schema"]).unwrap();
    let mut missing_config = workflow_mcp::discovery::examples().remove(0)["workflow"].clone();
    missing_config["nodes"][1]
        .as_object_mut()
        .unwrap()
        .remove("config");
    assert!(!validator.is_valid(&missing_config));
    assert!(
        workflow_service::definition::errors(&missing_config)
            .iter()
            .any(|e| e.path == "/nodes/1/config" && e.code == "REQUIRED_PROPERTY")
    );
    for example in workflow_mcp::discovery::examples() {
        assert!(
            validator.is_valid(&example["workflow"]),
            "{}: {:?}",
            example["id"],
            validator
                .iter_errors(&example["workflow"])
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            workflow_service::definition::validate(&example["workflow"])["valid"],
            true,
            "{}",
            example["id"]
        );
    }
    let listed = data(
        &mut ws,
        &mut auth,
        &token,
        "workflows_examples_list",
        json!({"limit":100}),
    );
    assert_eq!(
        listed["items"].as_array().unwrap().len(),
        workflow_mcp::discovery::examples().len()
    );
    assert!(
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e.get("workflow").is_none())
    );
    let connectors = data(&mut ws, &mut auth, &token, "connectors_list", json!({}));
    assert_eq!(connectors["connectors"].as_array().unwrap().len(), 27);
    assert!(!connectors.to_string().contains("[REDACTED]"));
    for connector in connectors["connectors"].as_array().unwrap() {
        assert!(
            !connector["examples"].as_array().unwrap().is_empty(),
            "{}",
            connector["kind"]
        );
        jsonschema::validator_for(&connector["configSchema"]).unwrap();
    }
    let capabilities = data(
        &mut ws,
        &mut auth,
        &token,
        "platform_capabilities_get",
        json!({}),
    );
    assert_eq!(capabilities["plannedConnectorsExecutable"], false);
    assert!(ws.workflows.is_empty());
    assert!(ws.runs.is_empty());
    assert!(ws.audit.is_empty());
    let operator = grant(&mut auth, "operator");
    let credential = auth.authenticate(&operator, RESOURCE, 1).unwrap();
    for tool in workflow_mcp::tools::list(&credential) {
        let validator = jsonschema::validator_for(&tool["inputSchema"]).unwrap();
        if ["workflows_create", "workflows_update", "workflows_validate"]
            .contains(&tool["name"].as_str().unwrap())
        {
            let example = workflow_mcp::discovery::examples().remove(0)["workflow"].clone();
            let mut args = json!({"workflow":example});
            if tool["name"] != "workflows_validate" {
                args["idempotencyKey"] = json!("tool-schema-check");
            }
            if tool["name"] == "workflows_update" {
                args["expectedVersion"] = json!(1);
            }
            assert!(validator.is_valid(&args), "{}", tool["name"]);
            args["workflow"]["nodes"][0]
                .as_object_mut()
                .unwrap()
                .remove("label");
            assert!(!validator.is_valid(&args));
        }
    }
}
#[test]
fn agents_can_repair_diagnostics_create_publish_child_and_simulate_parent() {
    let mut ws = Workspace::default();
    let mut auth = Auth::default();
    let token = grant(&mut auth, "operator");
    let mut workflow = data(
        &mut ws,
        &mut auth,
        &token,
        "workflows_examples_get",
        json!({"exampleId":"01-hello"}),
    )["workflow"]
        .clone();
    workflow["id"] = json!("agent-repaired");
    workflow["nodes"][0]
        .as_object_mut()
        .unwrap()
        .remove("label");
    workflow["nodes"][1]["version"] = json!("v1");
    let invalid = data(
        &mut ws,
        &mut auth,
        &token,
        "workflows_validate",
        json!({"workflow":workflow}),
    );
    assert_eq!(invalid["valid"], false);
    assert!(
        invalid["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["path"] == "/nodes/0/label" && e["code"] == "REQUIRED_PROPERTY")
    );
    assert!(
        invalid["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["path"] == "/nodes/1/version"
                && e["code"] == "INVALID_TYPE"
                && e["expected"] == "integer")
    );
    workflow["nodes"][0]["label"] = json!("Start");
    workflow["nodes"][1]["version"] = json!(1);
    let edge = workflow["edges"][0].clone();
    workflow["edges"][0]["sourcePort"] = json!("unknown");
    let invalid = data(
        &mut ws,
        &mut auth,
        &token,
        "workflows_validate",
        json!({"workflow":workflow}),
    );
    assert!(
        invalid["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["code"] == "INVALID_SOURCE_PORT" && e["path"] == "/edges/0/sourcePort")
    );
    workflow["edges"][0] = edge;
    assert_eq!(
        data(
            &mut ws,
            &mut auth,
            &token,
            "workflows_validate",
            json!({"workflow":workflow})
        )["valid"],
        true
    );
    data(
        &mut ws,
        &mut auth,
        &token,
        "workflows_create",
        json!({"workflow":workflow,"idempotencyKey":"create-repaired"}),
    );
    let run = data(
        &mut ws,
        &mut auth,
        &token,
        "runs_simulate",
        json!({"workflowId":"agent-repaired","expectedVersion":1,"input":{},"idempotencyKey":"simulate-repaired"}),
    );
    assert_eq!(run["status"], "succeeded");
    let parent = data(
        &mut ws,
        &mut auth,
        &token,
        "workflows_examples_get",
        json!({"exampleId":"19-invoke-workflow"}),
    );
    assert_eq!(parent["dependencies"], json!(["18-reusable-workflow"]));
    let child = data(
        &mut ws,
        &mut auth,
        &token,
        "workflows_examples_get",
        json!({"exampleId":"18-reusable-workflow"}),
    );
    for example in [&child, &parent] {
        assert_eq!(
            data(
                &mut ws,
                &mut auth,
                &token,
                "workflows_validate",
                json!({"workflow":example["workflow"]})
            )["valid"],
            true
        );
        data(
            &mut ws,
            &mut auth,
            &token,
            "workflows_create",
            json!({"workflow":example["workflow"],"idempotencyKey":format!("create-{}",example["id"].as_str().unwrap())}),
        );
        if example["id"] == "18-reusable-workflow" {
            data(
                &mut ws,
                &mut auth,
                &token,
                "workflows_publish",
                json!({"workflowId":"18-reusable-workflow","expectedVersion":1,"idempotencyKey":"publish-child"}),
            );
        }
    }
    let result = data(
        &mut ws,
        &mut auth,
        &token,
        "runs_simulate",
        json!({"workflowId":"19-invoke-workflow","expectedVersion":1,"input":{},"idempotencyKey":"simulate-parent"}),
    );
    assert_eq!(result["status"], "succeeded");
}
#[test]
fn config_diagnostics_do_not_echo_secret_values_and_invalid_shapes_never_save() {
    let mut ws = Workspace::default();
    let mut auth = Auth::default();
    let token = grant(&mut auth, "builder");
    let mut example = workflow_mcp::discovery::examples().remove(0)["workflow"].clone();
    example["nodes"][1]["type"] = json!("Aggregate");
    example["nodes"][1]["config"] = json!({"operation":"private-secret-should-not-leak"});
    let invalid = data(
        &mut ws,
        &mut auth,
        &token,
        "workflows_validate",
        json!({"workflow":example}),
    );
    assert_eq!(invalid["valid"], false);
    assert!(
        !invalid
            .to_string()
            .contains("private-secret-should-not-leak")
    );
    assert!(
        invalid["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["path"] == "/nodes/1/config/operation")
    );
    example["nodes"][1].as_object_mut().unwrap().remove("type");
    let result = call(
        &mut ws,
        &mut auth,
        &token,
        "workflows_create",
        json!({"workflow":example,"idempotencyKey":"invalid-cannot-create"}),
    );
    assert_eq!(result["isError"], true);
    assert!(
        result["structuredContent"]["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["path"] == "/nodes/1/type")
    );
    assert!(ws.workflows.is_empty());
}

#[test]
fn authoritative_schema_preserves_supported_encoded_and_dynamic_configurations() {
    let mut workflow = workflow_mcp::discovery::examples().remove(0)["workflow"].clone();
    for (kind, config) in [
        ("Split / Batch", json!({"batchSize":"10"})),
        (
            "Each / Loop",
            json!({"workflowId":"child","concurrency":"2"}),
        ),
        ("Split / Parallel", json!({"concurrency":"4"})),
        ("Wait / Delay", json!({"delayMs":"+1000"})),
        (
            "IF / Condition",
            json!({"conditions":"[{\"field\":\"$\",\"operator\":\"exists\"}]"}),
        ),
        (
            "Filter",
            json!({"conditions":"[{\"field\":\"$\",\"operator\":\"exists\"}]"}),
        ),
        (
            "Context Store",
            json!({"command":"{\"operation\":\"get\",\"tableId\":\"context\",\"key\":\"key\"}"}),
        ),
        (
            "HTTP Request",
            json!({"url":"https://example.com","method":"gEt"}),
        ),
        (
            "Webhook",
            json!({"contextHeaders":"[\"content-type\"]","contextQuery":"[]"}),
        ),
        (
            "Respond to Webhook",
            json!({"status":"201","headers":"{\"X-Result\":\"ok\"}"}),
        ),
        (
            "Respond to Webhook",
            json!({"status":{"$input":"/status"},"body":{"$input":""}}),
        ),
    ] {
        workflow["nodes"][1]["type"] = json!(kind);
        workflow["nodes"][1]["config"] = config;
        let errors = workflow_service::definition::schema_errors(&workflow);
        assert!(errors.is_empty(), "{kind}: {errors:?}");
    }
}

#[test]
fn connector_inspection_describes_runtime_values_without_reading_workspace_data() {
    let mut ws = Workspace::default();
    let mut auth = Auth::default();
    let token = grant(&mut auth, "reader");
    for manifest in workflow_service::manifests() {
        let detail = data(
            &mut ws,
            &mut auth,
            &token,
            "connectors_get",
            json!({"kind":manifest.kind}),
        );
        assert_ne!(
            detail["description"],
            workflow_service::contracts::UNDESCRIBED,
            "{} has no runtime contract",
            manifest.kind
        );
        assert!(detail.get("inputSchema").is_some());
        assert!(detail.get("outputSchema").is_some());
        jsonschema::validator_for(&detail["inputSchema"]).unwrap();
        jsonschema::validator_for(&detail["outputSchema"]).unwrap();
    }
    let sum = data(
        &mut ws,
        &mut auth,
        &token,
        "connectors_get",
        json!({"kind":"Aggregate","config":{"operation":"sum","arrayPointer":"/items"}}),
    );
    assert_eq!(sum["outputSchema"]["type"], "number");
    assert!(
        sum["examples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["config"].is_object())
    );
    assert_eq!(
        sum["examples"],
        workflow_mcp::discovery::connectors()["connectors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == "Aggregate")
            .unwrap()["examples"]
    );
    assert_eq!(sum["selectedInput"]["pointer"], "/items");
    assert!(sum["inputSchema"].as_object().unwrap().is_empty());
    let http = data(
        &mut ws,
        &mut auth,
        &token,
        "connectors_get",
        json!({"kind":"HTTP Request","config":{"responseType":"text"}}),
    );
    assert_eq!(http["outputSchema"]["properties"]["body"]["type"], "string");
    let mapping = data(
        &mut ws,
        &mut auth,
        &token,
        "connectors_get",
        json!({"kind":"Map / Transform","config":{"mapping":{"password":"caller-supplied-private-value","total":10,"customer":{"$input":"/customer"}}}}),
    );
    assert_eq!(
        mapping["outputSchema"]["properties"]["password"]["type"],
        "string"
    );
    assert!(
        !mapping
            .to_string()
            .contains("caller-supplied-private-value")
    );
    assert!(
        mapping["outputSchema"]["properties"]["customer"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    let declared = json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]});
    let trigger = data(
        &mut ws,
        &mut auth,
        &token,
        "connectors_get",
        json!({"kind":"Execute Workflow Trigger","config":{"outputNodeId":"done","inputSchema":declared,"outputSchema":{"type":"number"}}}),
    );
    assert_eq!(trigger["inputSchema"], declared);
    assert_eq!(trigger["outputSchema"], declared);
    assert_eq!(trigger["childContract"]["outputSchema"]["type"], "number");
    assert_eq!(trigger["childContract"]["outputNodeId"], "done");
    let encoded = workflow_service::contracts::describe(
        "Execute Workflow Trigger",
        json!({"outputNodeId":"done","inputSchema":declared.to_string()})
            .as_object()
            .unwrap(),
    );
    assert_eq!(encoded["inputSchema"], declared);
    assert!(
        encoded["childContract"]["outputSchema"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    for config in [json!({}), json!({"operation":"unknown"})] {
        let detail =
            workflow_service::contracts::describe("JSON Utilities", config.as_object().unwrap());
        assert!(detail["inputSchema"].as_object().unwrap().is_empty());
        assert!(detail["outputSchema"].as_object().unwrap().is_empty());
    }
    let parse = workflow_service::contracts::describe(
        "JSON Utilities",
        json!({"operation":"parse"}).as_object().unwrap(),
    );
    assert_eq!(parse["inputSchema"]["type"], "string");
    for example in workflow_mcp::discovery::examples() {
        for (id, expected) in example["expectedOutputs"].as_object().unwrap() {
            let node = example["workflow"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["id"] == *id)
                .unwrap();
            let detail = workflow_service::contracts::describe(
                node["type"].as_str().unwrap(),
                node["config"].as_object().unwrap(),
            );
            let validator = jsonschema::validator_for(&detail["outputSchema"]).unwrap();
            assert!(
                validator.is_valid(expected),
                "{} {id}: {:?}",
                example["id"],
                validator
                    .iter_errors(expected)
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
            );
        }
    }
    let nested = data(
        &mut ws,
        &mut auth,
        &token,
        "connectors_get",
        json!({"kind":"Map / Transform","config":{"mapping":"{\"auth\":{\"token\":\"nested-private-value\",\"retries\":[\"array-private-value\"]}}"}}),
    );
    assert_eq!(
        nested["outputSchema"]["properties"]["auth"]["properties"]["token"]["type"],
        "string"
    );
    let text = nested.to_string();
    assert!(!text.contains("nested-private-value"));
    assert!(!text.contains("array-private-value"));
    let expression = data(
        &mut ws,
        &mut auth,
        &token,
        "connectors_get",
        json!({"kind":"Expressions","config":{"fields":{"apiKey":{"op":"literal","value":"literal-private-value"}}}}),
    );
    assert_eq!(
        expression["outputSchema"]["properties"]["apiKey"]["type"],
        "string"
    );
    assert!(!expression.to_string().contains("literal-private-value"));
    assert!(ws.workflows.is_empty());
    assert!(ws.runs.is_empty());
}

#[test]
fn connector_inspection_rejects_invalid_versions_as_arguments() {
    let mut ws = Workspace::default();
    let mut auth = Auth::default();
    let token = grant(&mut auth, "reader");
    for version in [json!(0), json!(-1), json!("1"), json!(u64::MAX)] {
        let result = call(
            &mut ws,
            &mut auth,
            &token,
            "connectors_get",
            json!({"kind":"Aggregate","version":version}),
        );
        assert_eq!(result["isError"], true, "{version}");
        assert_eq!(
            result["structuredContent"]["code"], "invalid_arguments",
            "{version}"
        );
    }
    let missing = call(
        &mut ws,
        &mut auth,
        &token,
        "connectors_get",
        json!({"kind":"Aggregate","version":99}),
    );
    assert_eq!(
        missing["structuredContent"]["code"],
        "unsupported_connector"
    );
}
