use serde_json::{Value, json};
use workflow_service::{WebhookReply, Workspace};
fn graph(wait: u64, second_response: bool) -> Value {
    let mut nodes = vec![
        json!({"id":"start","type":"Webhook","label":"Webhook","config":{"authentication":"none"}}),
    ];
    if wait > 0 {
        nodes.push(
            json!({"id":"wait","type":"Wait / Delay","label":"Wait","config":{"delayMs":wait}}),
        );
    }
    nodes.push(json!({"id":"respond","type":"Respond to Webhook","label":"Respond","config":{"status":201,"body":{"$input":""}}}));
    nodes.push(if second_response {json!({"id":"after","type":"Respond to Webhook","label":"Duplicate response","config":{"body":{"duplicate":true}}})}else{json!({"id":"after","type":"Map / Transform","label":"Background","config":{"mapping":{"finished":true}}})});
    let edges: Vec<_> = nodes
        .windows(2)
        .enumerate()
        .map(|(i, n)| json!({"id":format!("e{i}"),"source":n[0]["id"],"target":n[1]["id"]}))
        .collect();
    json!({"schemaVersion":1,"id":"response","name":"Response test","status":"draft","updatedAt":"","nodes":nodes,"edges":edges})
}
fn start(wait: u64, duplicate: bool) -> Workspace {
    let mut ws = Workspace::default();
    ws.handle(
        "PUT",
        "/api/workflows/response",
        &serde_json::to_vec(&graph(wait, duplicate)).unwrap(),
        0,
        "",
    )
    .unwrap();
    ws.configure_webhook("response", "endpoint".into(), "hash".into())
        .unwrap();
    ws.handle("POST", "/api/workflows/response/publish", b"", 0, "")
        .unwrap();
    ws.receive_webhook_context(
        "response",
        "endpoint",
        json!({"input":"original"}),
        0,
        "run",
        json!({"trigger":"webhook","request":{}}),
    )
    .unwrap();
    ws
}
#[test]
fn response_is_pass_through_and_background_progress_follows_submission() {
    let mut ws = start(0, false);
    let run = ws.runs[0].view(false).unwrap();
    assert_eq!(run["nodes"][1]["output"], json!({"input":"original"}));
    assert_eq!(run["nodes"][2]["status"], "pending");
    let mut ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    let WebhookReply::Response(spec) = ws.webhook_reply("run", 0, false).unwrap() else {
        panic!("Expected response")
    };
    assert_eq!(spec.status, 201);
    assert_eq!(spec.body, json!({"input":"original"}));
    let mut ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    assert!(matches!(
        ws.webhook_reply("run", 0, false).unwrap(),
        WebhookReply::Unavailable
    ));
    ws.alarm(1).unwrap();
    assert_eq!(ws.runs[0].view(false).unwrap()["status"], "succeeded");
    assert_eq!(
        ws.runs[0].view(false).unwrap()["webhookResponse"]["delivery"],
        "submitted"
    );
}
#[test]
fn late_response_and_disconnect_do_not_cancel_the_workflow() {
    for disconnected in [false, true] {
        let mut ws = start(6000, false);
        assert_eq!(ws.next_alarm(), Some(5000));
        if disconnected {
            assert!(matches!(
                ws.webhook_reply("run", 1, true).unwrap(),
                WebhookReply::Disconnected
            ));
        } else {
            ws.alarm(5000).unwrap();
            assert!(matches!(
                ws.webhook_reply("run", 5000, false).unwrap(),
                WebhookReply::TimedOut
            ));
        }
        let mut ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
        ws.alarm(6000).unwrap();
        ws.alarm(6001).unwrap();
        let run = ws.runs[0].view(false).unwrap();
        assert_eq!(run["status"], "succeeded");
        assert_eq!(run["webhookResponse"]["decision"]["late"], true);
        assert_eq!(
            run["webhookResponse"]["delivery"],
            if disconnected {
                "disconnected"
            } else {
                "timed_out"
            }
        );
    }
}
#[test]
fn second_response_is_a_workflow_error_and_preview_never_submits() {
    let mut ws = start(0, true);
    let _ = ws.webhook_reply("run", 0, false).unwrap();
    ws.alarm(1).unwrap();
    let run = ws.runs[0].view(false).unwrap();
    assert_eq!(run["status"], "failed");
    assert_eq!(run["nodes"][2]["error"], "duplicate_response");
    assert_eq!(run["webhookResponse"]["decision"]["nodeId"], "respond");
    let mut ws = Workspace::default();
    ws.handle(
        "PUT",
        "/api/workflows/response",
        &serde_json::to_vec(&graph(0, false)).unwrap(),
        0,
        "",
    )
    .unwrap();
    let (_, run) = ws
        .handle(
            "POST",
            "/api/runs",
            br#"{"workflowId":"response","mode":"simulation","input":{}}"#,
            0,
            "preview",
        )
        .unwrap();
    assert_eq!(run["status"], "succeeded");
    assert_eq!(run["webhookResponse"]["delivery"], "preview");
    assert!(run["webhookResponse"]["deadlineMs"].is_null());
}
