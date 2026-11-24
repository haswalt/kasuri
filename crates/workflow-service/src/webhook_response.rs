use crate::{ApiError, RunRecord, Workspace, error};
use connectors_webhook::response::{RESPONSE_WINDOW_MS, ResponseSpec};
use serde::{Deserialize, Serialize};
use workflow_engine::RunStatus;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    Waiting,
    Ready,
    Submitted,
    TimedOut,
    Disconnected,
    NoResponse,
    Failed,
    Preview,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub node_id: String,
    pub prepared_at: u64,
    pub late: bool,
    pub response: ResponseSpec,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseWindow {
    pub deadline_ms: Option<u64>,
    pub delivery: Delivery,
    pub closed_at: Option<u64>,
    pub decision: Option<Decision>,
}
/// What the handler holding a webhook's request should do with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "reply", content = "response")]
pub enum Reply {
    Pending,
    Response(ResponseSpec),
    TimedOut,
    Disconnected,
    NoResponse,
    Failed,
    Unavailable,
}
impl ResponseWindow {
    pub fn new(live: bool, now: u64) -> Self {
        Self {
            deadline_ms: live.then(|| now.saturating_add(RESPONSE_WINDOW_MS)),
            delivery: if live {
                Delivery::Waiting
            } else {
                Delivery::Preview
            },
            closed_at: None,
            decision: None,
        }
    }
    pub fn open(&self) -> bool {
        matches!(self.delivery, Delivery::Waiting | Delivery::Ready)
    }
    pub fn expire(&mut self, now: u64) {
        if self.open() && self.deadline_ms.is_some_and(|at| now >= at) {
            self.delivery = Delivery::TimedOut;
            self.closed_at = self.deadline_ms;
        }
    }
    pub fn prepare(&mut self, node_id: &str, response: ResponseSpec, now: u64) -> bool {
        if self.decision.is_some() {
            return false;
        }
        self.expire(now);
        self.decision = Some(Decision {
            node_id: node_id.into(),
            prepared_at: now,
            late: !self.open() && self.delivery != Delivery::Preview,
            response,
        });
        if self.open() {
            self.delivery = Delivery::Ready
        }
        true
    }
}
impl RunRecord {
    pub(crate) fn settle_response(&mut self, now: u64) -> Result<(), ApiError> {
        let status = self.status()?;
        if let Some(window) = &mut self.webhook_response {
            window.expire(now);
            if window.delivery == Delivery::Waiting
                && matches!(
                    status,
                    RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
                )
            {
                window.delivery = if status == RunStatus::Succeeded {
                    Delivery::NoResponse
                } else {
                    Delivery::Failed
                };
                window.closed_at = Some(now);
            }
        }
        Ok(())
    }
}
impl Workspace {
    /// Called only by the handler owning the original HTTP request. Submission is
    /// durable before the response leaves Rust; it does not prove client receipt.
    pub fn webhook_reply(
        &mut self,
        id: &str,
        now: u64,
        disconnected: bool,
    ) -> Result<Reply, ApiError> {
        let Some(record) = self.runs.iter_mut().find(|r| r.matches_id(id)) else {
            // An archived run's window has already closed, so its reply only needs
            // reading: nothing about it changes.
            let mut record = self
                .find_run(id)?
                .ok_or_else(|| error(404, "Run not found"))?
                .into_owned();
            record.settle_response(now)?;
            return Ok(match record.webhook_response.map(|w| w.delivery) {
                Some(Delivery::TimedOut) => Reply::TimedOut,
                Some(Delivery::Disconnected) => Reply::Disconnected,
                Some(Delivery::NoResponse) => Reply::NoResponse,
                Some(Delivery::Failed) => Reply::Failed,
                _ => Reply::Unavailable,
            });
        };
        record.settle_response(now)?;
        let Some(window) = &mut record.webhook_response else {
            return Ok(Reply::Unavailable);
        };
        if disconnected && window.open() {
            window.delivery = Delivery::Disconnected;
            window.closed_at = Some(now);
        }
        Ok(match window.delivery {
            Delivery::Ready => {
                window.delivery = Delivery::Submitted;
                window.closed_at = Some(now);
                Reply::Response(window.decision.as_ref().unwrap().response.clone())
            }
            Delivery::Waiting => Reply::Pending,
            Delivery::TimedOut => Reply::TimedOut,
            Delivery::Disconnected => Reply::Disconnected,
            Delivery::NoResponse => Reply::NoResponse,
            Delivery::Failed => Reply::Failed,
            _ => Reply::Unavailable,
        })
    }
}
