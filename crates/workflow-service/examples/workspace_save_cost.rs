//! Native measurement of resident-workspace change detection and commit CPU.
//! cargo run --release -p workflow-service --example workspace_save_cost -- --samples 25
//! Setup/load/fixture construction is excluded. JSON lines are suitable for before/after comparisons.
use serde_json::json;
use std::{hint::black_box, time::Instant};
use workflow_service::store::{MemoryStore, Tracked};
use workflow_service::{Document, MAX_STATE_BYTES, Revision, Workspace};
fn document(id: String) -> Document {
    serde_json::from_value(json!({"schemaVersion":1,"id":id,"name":"W","status":"draft","updatedAt":"","lifecycle":{"publishedRevision":null,"enabled":false,"needsReview":false},"nodes":[{"id":"s","type":"Manual Trigger","label":"S","config":{}}],"edges":[]})).unwrap()
}
fn fixture(count: usize, profile: &str) -> Workspace {
    let mut workspace = Workspace::default();
    if profile == "workflows" {
        workspace.workflows = (0..count).map(|i| document(format!("w{i}"))).collect();
    } else {
        let doc = document("w".into());
        workspace.revisions = (1..=count)
            .map(|i| Revision {
                workflow_id: "w".into(),
                revision: u32::try_from(i).unwrap(),
                graph: doc.graph.clone(),
                fingerprint: "a".repeat(64),
                published_at: "2026-11-25T00:00:00Z".into(),
                published_by: "benchmark".into(),
                legacy_baseline: false,
            })
            .collect();
        workspace.workflows.push(doc);
    }
    workspace
}
fn percentile(values: &mut [u128], percent: usize) -> u128 {
    values.sort_unstable();
    values[(values.len() - 1) * percent / 100]
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let samples = args
        .iter()
        .position(|a| a == "--samples")
        .map(|i| args[i + 1].parse::<usize>().expect("positive sample count"))
        .unwrap_or(25);
    assert!(samples > 0);
    for profile in ["workflows", "revisions"] {
        for count in [100, 1000, 10000] {
            let mut store = MemoryStore::default();
            let mut tracked = Tracked::load(&store).unwrap();
            tracked.workspace = fixture(count, profile);
            let bytes = serde_json::to_vec(&tracked.workspace).unwrap().len();
            assert!(
                bytes <= MAX_STATE_BYTES,
                "{profile}/{count} exceeds the current resident workspace budget ({bytes} bytes)"
            );
            tracked.commit(&mut store).unwrap();
            for change in [false, true] {
                let mut detection = Vec::new();
                let mut commits = Vec::new();
                let mut written_bytes = 0;
                let mut written_rows = 0;
                // Commit alternating descriptions so each mutation sample writes exactly one workflow.
                for sample in 0..samples + 3 {
                    if change {
                        tracked.workspace.workflows[0].graph.description = format!("edit-{sample}");
                    }
                    let start = Instant::now();
                    let changes = black_box(tracked.changes().unwrap());
                    let elapsed = start.elapsed().as_nanos();
                    assert_eq!(changes.upserts.len(), usize::from(change));
                    assert!(changes.deletes.is_empty());
                    assert!(changes.meta.is_none());
                    let start = Instant::now();
                    let changes = black_box(tracked.commit(&mut store).unwrap());
                    let commit_elapsed = start.elapsed().as_nanos();
                    assert_eq!(changes.upserts.len(), usize::from(change));
                    if sample >= 3 {
                        detection.push(elapsed);
                        commits.push(commit_elapsed);
                        written_bytes = changes.upserts.iter().map(|r| r.body.len()).sum::<usize>();
                        written_rows = changes.upserts.len();
                    }
                }
                println!(
                    "{}",
                    json!({"profile":profile,"entities":count,"residentWorkflows":tracked.workspace.workflows.len(),"residentRevisions":tracked.workspace.revisions.len(),"residentBytes":bytes,"operation":if change{"one_workflow_edit"}else{"no_op"},"samples":samples,"changeDetectionMedianNs":percentile(&mut detection,50),"changeDetectionP95Ns":percentile(&mut detection,95),"commitMedianNs":percentile(&mut commits,50),"commitP95Ns":percentile(&mut commits,95),"upserts":written_rows,"writtenBodyBytes":written_bytes,"measurement":"Native release build; change detection includes retention, encoding, ordering and digests. Commit includes MemoryStore apply but excludes SQL, host cloning, request processing and setup."})
                );
            }
        }
    }
}
