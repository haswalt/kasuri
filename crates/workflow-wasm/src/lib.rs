//! JSON-only Wasm boundary around the actual Rust engine (no JavaScript reimplementation).
use connector_sdk::Connector;
use connectors_core::CoreConnector;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use workflow_engine::{Dispatch, Limits, Run, RunStatus};
use workflow_schema::Workflow;

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[wasm_bindgen]
pub fn manifests() -> String {
    serde_json::to_string(&connectors_core::manifests()).unwrap()
}

#[wasm_bindgen]
pub fn validate(workflow_json: &str) -> Result<(), JsValue> {
    Workflow::from_json(workflow_json)
        .map(|_| ())
        .map_err(js_error)
}

#[wasm_bindgen]
pub fn start(
    id: &str,
    workflow_json: &str,
    input_json: &str,
    now_ms: f64,
) -> Result<String, JsValue> {
    let workflow = Workflow::from_json(workflow_json).map_err(js_error)?;
    let input: Value =
        serde_json::from_str(input_json).map_err(|_| JsValue::from_str("Invalid input JSON"))?;
    Run::new(
        id,
        workflow,
        input,
        timestamp(now_ms)?,
        Limits {
            max_dispatches: 250,
            max_duration_ms: 86_400_000,
        },
    )
    .and_then(|run| run.checkpoint())
    .map_err(js_error)
}
fn timestamp(now: f64) -> Result<u64, JsValue> {
    if !now.is_finite() || now < 0.0 || now.fract() != 0.0 || now > 9_007_199_254_740_991.0 {
        return Err(JsValue::from_str("Invalid timestamp"));
    }
    Ok(now as u64)
}

/// Run a bounded batch of core tasks. A real host persists the returned checkpoint
/// and schedules wakeAt; simulation hosts advance a virtual clock to that timestamp.
#[wasm_bindgen]
pub fn advance(checkpoint: &str, now_ms: f64) -> Result<String, JsValue> {
    let mut run = Run::restore(checkpoint).map_err(js_error)?;
    let now = timestamp(now_ms)?;
    let mut wake_at = None;
    for _ in 0..20 {
        match run.next(now).map_err(js_error)? {
            Dispatch::Task(task) => {
                run.complete(&task.id, CoreConnector.execute(&task), now)
                    .map_err(js_error)?;
            }
            Dispatch::SleepUntil(at) => {
                wake_at = Some(at);
                break;
            }
            Dispatch::Paused | Dispatch::Finished(_) => break,
        }
    }
    if run.status() == RunStatus::Running && wake_at.is_none() {
        wake_at = Some(now.saturating_add(1));
    }
    Ok(json!({"checkpoint":run.checkpoint().map_err(js_error)?,"wakeAt":wake_at}).to_string())
}

#[wasm_bindgen]
pub fn control(checkpoint: &str, action: &str) -> Result<String, JsValue> {
    let mut run = Run::restore(checkpoint).map_err(js_error)?;
    match action {
        "pause" => run.pause(),
        "resume" => run.resume(),
        "cancel" => run.cancel(),
        _ => return Err(JsValue::from_str("Unknown run action")),
    }
    .map_err(js_error)?;
    run.checkpoint().map_err(js_error)
}
