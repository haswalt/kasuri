//! Draft edits and immutable trigger definitions have independent lifecycles.
use crate::{ApiError, Document, Publication, Workspace, error, iso};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use workflow_schema::Workflow;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lifecycle {
    pub published_revision: Option<u32>,
    pub enabled: bool,
    pub needs_review: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub workflow_id: String,
    pub revision: u32,
    pub graph: Workflow,
    pub fingerprint: String,
    pub published_at: String,
    #[serde(default)]
    pub published_by: String,
    pub legacy_baseline: bool,
}
pub fn fingerprint(graph: &Workflow) -> String {
    let mut graph = graph.clone();
    graph.revision = 0;
    graph.nodes.sort_by(|a, b| a.id.cmp(&b.id));
    for node in &mut graph.nodes {
        node.position = None;
    }
    // Connection IDs are editor identities, not executable content.
    for edge in &mut graph.edges {
        edge.id.clear();
    }
    graph.edges.sort_by(|a, b| {
        (&a.source, &a.source_port, &a.target).cmp(&(&b.source, &b.source_port, &b.target))
    });
    connectors_webhook::hex(&Sha256::digest(
        serde_json::to_vec(&graph).expect("serializable workflow"),
    ))
}
impl Workspace {
    pub(crate) fn migrate_publications(&mut self) {
        for doc in &mut self.workflows {
            if doc.lifecycle.is_some() {
                continue;
            }
            let known_live = doc.graph.revision > 0 && doc.status != Publication::Draft;
            doc.lifecycle = Some(Lifecycle {
                published_revision: known_live.then_some(doc.graph.revision),
                enabled: known_live && doc.status == Publication::Active,
                needs_review: doc.graph.revision > 0 && !known_live,
            });
            doc.has_unpublished_changes = !known_live;
            if !known_live {
                doc.status = Publication::Draft;
                doc.next_scheduled_at = None;
            }
            if known_live
                && !self
                    .revisions
                    .iter()
                    .any(|r| r.workflow_id == doc.graph.id && r.revision == doc.graph.revision)
            {
                self.revisions.push(Revision {
                    workflow_id: doc.graph.id.clone(),
                    revision: doc.graph.revision,
                    graph: doc.graph.clone(),
                    fingerprint: fingerprint(&doc.graph),
                    published_at: String::new(),
                    published_by: "legacy".into(),
                    legacy_baseline: true,
                });
            }
        }
    }
    pub fn live_document(&self, id: &str) -> Result<Document, ApiError> {
        let mut doc = self.workflows[self.index(id)?].clone();
        if doc.archived_at.is_some() {
            return Err(error(404, "Workflow is in trash"));
        }
        let revision = doc
            .lifecycle
            .as_ref()
            .and_then(|l| l.published_revision)
            .ok_or_else(|| error(409, "Workflow has no verified published revision"))?;
        doc.graph = self
            .revisions
            .iter()
            .find(|r| r.workflow_id == id && r.revision == revision)
            .ok_or_else(|| error(500, "Published revision missing"))?
            .graph
            .clone();
        Ok(doc)
    }
    pub(crate) fn publish(
        &mut self,
        index: usize,
        now: u64,
        actor: &str,
    ) -> Result<Document, ApiError> {
        let mut doc = self.workflows[index].clone();
        Self::executable(&doc)?;
        let hash = fingerprint(&doc.graph);
        if let Some(live) = doc.lifecycle.as_ref().and_then(|l| l.published_revision) {
            if self.revisions.iter().any(|r| {
                r.workflow_id == doc.graph.id && r.revision == live && r.fingerprint == hash
            }) {
                return Ok(doc); // Publishing unchanged content must not resume a paused workflow.
            }
        }
        doc.graph.revision = doc
            .graph
            .revision
            .checked_add(1)
            .ok_or_else(|| error(409, "Revision limit reached"))?;
        doc.lifecycle = Some(Lifecycle {
            published_revision: Some(doc.graph.revision),
            enabled: true,
            needs_review: false,
        });
        doc.has_unpublished_changes = false;
        doc.status = Publication::Active;
        doc.next_scheduled_at = crate::schedule(&doc.graph, now)?.map(|s| s.at);
        doc.updated_at = iso(now);
        doc.version = doc
            .version
            .checked_add(1)
            .ok_or_else(|| error(409, "Version exhausted"))?;
        self.revisions.push(Revision {
            workflow_id: doc.graph.id.clone(),
            revision: doc.graph.revision,
            graph: doc.graph.clone(),
            fingerprint: hash,
            published_at: iso(now),
            published_by: actor.into(),
            legacy_baseline: false,
        });
        self.workflows[index] = doc.clone();
        Ok(doc)
    }
}
