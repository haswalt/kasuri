//! Local demonstration host using a virtual clock, no external services or real sleeps.
use connector_sdk::Connector;
use connectors_core::CoreConnector;
use serde_json::Value;
use workflow_engine::{Dispatch, Limits, Run, RunStatus};
use workflow_schema::Workflow;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("Usage: run <workflow.json> <input.json>".into());
    }
    let workflow = Workflow::from_json(&std::fs::read_to_string(&args[1])?)?;
    let input: Value = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
    let mut now = 0;
    let mut run = Run::new("local-example", workflow, input, now, Limits::default())?;
    loop {
        let dispatch = run.next(now)?;
        // A production host must DURABLY persist this before performing a side effect.
        // Round-tripping here exercises recovery without claiming durable persistence.
        run = Run::restore(&run.checkpoint()?)?;
        match dispatch {
            Dispatch::Task(task) => {
                println!("Executing {} (attempt {})", task.node.label, task.attempt);
                run.complete(&task.id, CoreConnector.execute(&task), now)?;
                run = Run::restore(&run.checkpoint()?)?;
            }
            Dispatch::SleepUntil(at) => {
                println!("Advancing virtual clock to {at} ms");
                now = at;
            }
            Dispatch::Finished(status) => {
                println!("Run: {status:?} ({} dispatches)", run.dispatches());
                if status != RunStatus::Succeeded {
                    return Err("Run did not succeed".into());
                }
                println!("{}", serde_json::to_string_pretty(run.nodes())?);
                break;
            }
            Dispatch::Paused => return Err("Run unexpectedly paused".into()),
        }
    }
    Ok(())
}
