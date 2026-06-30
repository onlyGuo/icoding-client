use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PROTOCOL_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T = Value> {
    pub id: String,
    #[serde(rename = "type")]
    pub message_type: String,
    #[serde(default)]
    pub timestamp: Value,
    pub protocol_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    pub payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequestPayload {
    pub task_id: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    pub capability: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub require_result: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub details: Value,
}

impl<T> Envelope<T> {
    pub fn new(message_type: impl Into<String>, payload: T) -> Self {
        Self {
            id: new_message_id(),
            message_type: message_type.into(),
            timestamp: json!(Utc::now().to_rfc3339()),
            protocol_version: PROTOCOL_VERSION.to_string(),
            reply_to: None,
            payload,
        }
    }

    pub fn reply_to(mut self, message_id: impl Into<String>) -> Self {
        self.reply_to = Some(message_id.into());
        self
    }
}

pub fn client_ack(message_id: &str, task_id: Option<&str>, status: &str) -> Envelope<Value> {
    Envelope::new(
        "client.ack",
        json!({
            "message_id": message_id,
            "task_id": task_id,
            "status": status
        }),
    )
}

pub fn task_accepted(reply_to: &str, task_id: &str) -> Envelope<Value> {
    Envelope::new(
        "task.accepted",
        json!({
            "task_id": task_id,
            "status": "accepted"
        }),
    )
    .reply_to(reply_to)
}

pub fn task_result(task_id: &str, result: Value, duration_ms: u128) -> Envelope<Value> {
    Envelope::new(
        "task.result",
        json!({
            "task_id": task_id,
            "status": "completed",
            "duration_ms": duration_ms,
            "result": result
        }),
    )
}

pub fn task_failed(
    task_id: &str,
    code: &str,
    message: impl Into<String>,
    details: Value,
) -> Envelope<Value> {
    Envelope::new(
        "task.failed",
        json!({
            "task_id": task_id,
            "status": "failed",
            "error": {
                "code": code,
                "message": message.into(),
                "details": details
            }
        }),
    )
}

pub fn task_rejected(
    reply_to: &str,
    task_id: &str,
    code: &str,
    message: impl Into<String>,
    details: Value,
) -> Envelope<Value> {
    Envelope::new(
        "task.rejected",
        json!({
            "task_id": task_id,
            "status": "rejected",
            "error": {
                "code": code,
                "message": message.into(),
                "details": details
            }
        }),
    )
    .reply_to(reply_to)
}

pub fn client_goodbye(session_id: Option<&str>, reason: &str) -> Envelope<Value> {
    Envelope::new(
        "client.goodbye",
        json!({
            "session_id": session_id,
            "reason": reason
        }),
    )
}

pub fn client_ping(session_id: Option<&str>) -> Envelope<Value> {
    Envelope::new("client.ping", json!({ "session_id": session_id }))
}

pub fn new_message_id() -> String {
    format!("msg_{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_failure_includes_specific_details() {
        let envelope = task_failed(
            "task-1",
            "NOT_FOUND",
            "fs.list failed: directory does not exist",
            json!({
                "capability": "fs.list",
                "causes": ["directory does not exist"]
            }),
        );

        assert_eq!(envelope.payload["error"]["code"], "NOT_FOUND");
        assert_eq!(
            envelope.payload["error"]["details"]["capability"],
            "fs.list"
        );
        assert_eq!(
            envelope.payload["error"]["details"]["causes"][0],
            "directory does not exist"
        );
    }
}
