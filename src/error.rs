use anyhow::{Error, Result, bail};
use reqwest::Response;
use std::io;

pub fn message(error: &Error) -> String {
    format!("{error:#}")
}

pub fn causes(error: &Error) -> Vec<String> {
    error.chain().skip(1).map(ToString::to_string).collect()
}

pub async fn ensure_http_success(response: Response, action: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let body = response
        .text()
        .await
        .unwrap_or_else(|error| format!("failed to read server error response: {error}"));
    let reason = response_reason(&body).unwrap_or_else(|| {
        status
            .canonical_reason()
            .unwrap_or("unknown server error")
            .to_string()
    });
    bail!("{action} failed: server returned HTTP {status}: {reason}")
}

pub fn code(error: &Error) -> &'static str {
    for cause in error.chain() {
        if let Some(error) = cause.downcast_ref::<io::Error>() {
            return match error.kind() {
                io::ErrorKind::NotFound => "NOT_FOUND",
                io::ErrorKind::PermissionDenied => "PERMISSION_DENIED",
                io::ErrorKind::AlreadyExists => "ALREADY_EXISTS",
                io::ErrorKind::TimedOut => "TIMEOUT",
                io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => "INVALID_ARGUMENT",
                _ => "IO_ERROR",
            };
        }
        if cause.downcast_ref::<serde_json::Error>().is_some() {
            return "INVALID_ARGUMENT";
        }
        if let Some(error) = cause.downcast_ref::<reqwest::Error>() {
            if error.is_timeout() {
                return "TIMEOUT";
            }
            if error.status() == Some(reqwest::StatusCode::UNAUTHORIZED) {
                return "AUTH_REQUIRED";
            }
            return "NETWORK_ERROR";
        }
    }

    let message = message(error).to_ascii_lowercase();
    if message.contains("http 401") {
        "AUTH_REQUIRED"
    } else if message.contains("http 403") {
        "PERMISSION_DENIED"
    } else if message.contains("http 404") {
        "NOT_FOUND"
    } else if message.contains("http 409") {
        "CONFLICT"
    } else if message.contains("http 429") {
        "LIMIT_EXCEEDED"
    } else if message.contains("outside allowed roots")
        || message.contains("blocked by policy")
        || message.contains("disabled by local policy")
    {
        "POLICY_DENIED"
    } else if message.contains("not logged in") || message.contains("reauthentication") {
        "AUTH_REQUIRED"
    } else if message.contains("too large") || message.contains("limit") {
        "LIMIT_EXCEEDED"
    } else if message.contains("already exists") || message.contains("changed before write") {
        "CONFLICT"
    } else if message.contains("unsupported") {
        "UNSUPPORTED"
    } else if message.contains("invalid")
        || message.contains("is required")
        || message.contains("is empty")
        || message.contains("not a directory")
        || message.contains("not a file")
    {
        "INVALID_ARGUMENT"
    } else {
        "TASK_FAILED"
    }
}

fn response_reason(body: &str) -> Option<String> {
    let body = body.trim();
    if body.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        for field in ["message", "error", "detail", "reason"] {
            if let Some(value) = value.get(field) {
                if let Some(message) = value.as_str() {
                    return Some(truncate(message, 512));
                }
                if !value.is_null() {
                    return Some(truncate(&value.to_string(), 512));
                }
            }
        }
    }

    Some(truncate(body, 512))
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_context_and_root_cause() {
        let error = Error::new(io::Error::from(io::ErrorKind::NotFound))
            .context("failed to read directory /missing")
            .context("fs.list failed");

        assert_eq!(code(&error), "NOT_FOUND");
        assert!(message(&error).contains("fs.list failed"));
        assert!(message(&error).contains("failed to read directory /missing"));
        assert!(!causes(&error).is_empty());
    }

    #[test]
    fn classifies_policy_failures() {
        let error = anyhow::anyhow!("path is outside allowed roots: /private/file");
        assert_eq!(code(&error), "POLICY_DENIED");
    }

    #[test]
    fn extracts_json_server_reason() {
        assert_eq!(
            response_reason(r#"{"message":"verification code expired"}"#).as_deref(),
            Some("verification code expired")
        );
    }
}
