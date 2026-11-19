//! Contextual management evidence. Execution, payloads and configuration values
//! belong to their own stores and never enter management audit records.
use crate::{Document, Workspace};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn reference(doc: &Document) -> Value {
    json!({"version":doc.version,"revision":doc.graph.revision,"publishedRevision":doc.lifecycle.as_ref().and_then(|l|l.published_revision),"enabled":doc.lifecycle.as_ref().is_some_and(|l|l.enabled),"archived":doc.archived_at.is_some()})
}
fn changes(before: Option<&Document>, after: Option<&Document>) -> Vec<Value> {
    let Some(after) = after else { return vec![] };
    let Some(before) = before else {
        return vec![
            json!({"field":"workflow","description":format!("Created with {} nodes and {} connections",after.graph.nodes.len(),after.graph.edges.len())}),
        ];
    };
    let mut changes = Vec::new();
    if before.graph.name != after.graph.name {
        changes.push(json!({"field":"name","before":before.graph.name,"after":after.graph.name}));
    }
    if before.graph.description != after.graph.description {
        changes.push(json!({"field":"description","description":"Description changed"}));
    }
    let old: BTreeMap<_, _> = before.graph.nodes.iter().map(|n| (&n.id, n)).collect();
    let new: BTreeMap<_, _> = after.graph.nodes.iter().map(|n| (&n.id, n)).collect();
    for (id, node) in &new {
        match old.get(id) {
            None=>changes.push(json!({"field":"nodes","nodeId":id,"nodeName":node.label,"description":format!("Added {} node",node.kind)})),
            Some(previous) if *previous != *node => {
                let mut fields=Vec::new();
                if previous.kind!=node.kind {fields.push("type");}
                if previous.label!=node.label {fields.push("label");}
                if previous.config!=node.config {fields.push("configuration");}
                if previous.retry!=node.retry {fields.push("retry policy");}
                if previous.timeout_ms!=node.timeout_ms {fields.push("timeout");}
                if previous.version!=node.version {fields.push("connector version");}
                if previous.position!=node.position {fields.push("canvas position");}
                changes.push(json!({"field":"nodes","nodeId":id,"nodeName":node.label,"description":format!("Changed {}",fields.join(", "))}));
            }
            _=>{}
        }
    }
    for (id, node) in &old {
        if !new.contains_key(id) {
            changes.push(json!({"field":"nodes","nodeId":id,"nodeName":node.label,"description":"Removed node"}));
        }
    }
    if before.graph.edges != after.graph.edges {
        changes.push(json!({"field":"connections","beforeCount":before.graph.edges.len(),"afterCount":after.graph.edges.len(),"description":"Connections changed"}));
    }
    if before
        .lifecycle
        .as_ref()
        .map(|l| (l.published_revision, l.enabled))
        != after
            .lifecycle
            .as_ref()
            .map(|l| (l.published_revision, l.enabled))
        || before.archived_at != after.archived_at
    {
        changes.push(
            json!({"field":"lifecycle","description":"Publication or activation state changed"}),
        );
    }
    if changes.len() > 32 {
        let total = changes.len();
        changes.truncate(32);
        changes.push(json!({"field":"summary","description":format!("{} additional changes omitted",total-32)}));
    }
    changes
}
pub(crate) fn visible(event: &Value) -> bool {
    if event["schemaVersion"] == 2 {
        return true;
    }
    let action = event["action"].as_str().unwrap_or_default();
    !action.starts_with("runs")
        && !matches!(
            event["actor"].as_str(),
            Some("engine" | "scheduler" | "webhook")
        )
        && (action.starts_with("mcp.credential")
            || action == "oauth.consent"
            || event.get("before").is_some_and(|v| !v.is_null())
            || event.get("after").is_some_and(|v| !v.is_null()))
}
impl Workspace {
    pub(crate) fn workflow_audit(
        &mut self,
        actor: &str,
        action: &str,
        id: &str,
        before: Option<&Document>,
        after: Option<&Document>,
        status: Option<u16>,
        now: u64,
        correlation: &str,
    ) {
        if status.is_none() && before.map(|d| d.version) == after.map(|d| d.version) {
            return;
        }
        let name = after
            .or(before)
            .map(|d| d.graph.name.as_str())
            .unwrap_or(id);
        self.record_audit(
            actor,
            action,
            id,
            if status.is_some() {
                "rejected"
            } else {
                "succeeded"
            },
            now,
            correlation,
        );
        if let Some(event) = self.audit.last_mut() {
            event["entity"] = json!({"type":"workflow","id":id,"name":name});
            event["summary"] = json!(format!(
                "{}: {}",
                name,
                action.replace("workflow.", "").replace('_', " ")
            ));
            event["before"] = before.map(reference).unwrap_or(Value::Null);
            event["after"] = after.map(reference).unwrap_or(Value::Null);
            event["changes"] = json!(if status.is_none() {
                changes(before, after)
            } else {
                vec![]
            });
            if let Some(status) = status {
                event["reason"] = json!({"status":status,"description":match status {409=>"Edit precondition conflict",428=>"Missing edit precondition",400=>"Validation rejected",_=>"Management operation rejected"}});
            }
        }
    }
    pub fn audit_recent_page(&self, before: u64, limit: usize) -> Value {
        let items: Vec<_> = self
            .audit
            .iter()
            .rev()
            .filter(|v| visible(v) && (before == 0 || v["id"].as_u64().unwrap_or(0) < before))
            .take(limit.clamp(1, 100))
            .cloned()
            .collect();
        json!({"items":items,"nextCursor":items.last().map(|v|&v["id"]),"oldestRetainedId":self.audit.iter().find(|v|visible(v)).map(|v|&v["id"]),"totalRecorded":self.audit_sequence,"retentionLimit":1000})
    }
}
