use crate::{
    auth::Session,
    capabilities::CapabilityDispatcher,
    config::AppConfig,
    device::DeviceRegisterRequest,
    error::{causes as error_causes, code as error_code, message as error_message},
    protocol::{
        Envelope, TaskRequestPayload, client_ack, client_goodbye, client_ping, task_accepted,
        task_failed, task_rejected, task_result,
    },
};
use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use reqwest::Url;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderValue, header::AUTHORIZATION},
    },
};
use tracing::{error, info, warn};

pub struct AgentWsClient {
    config: AppConfig,
    session: Session,
    ws_url: String,
    connection_token: Option<String>,
    dispatcher: CapabilityDispatcher,
}

impl AgentWsClient {
    pub fn new(
        config: AppConfig,
        session: Session,
        ws_url: Option<String>,
        connection_token: Option<String>,
    ) -> Self {
        let ws_url = ws_url
            .filter(|url| !url.trim().is_empty())
            .map(|url| resolve_ws_url(&config, &url))
            .unwrap_or_else(|| config.server.ws_url.clone());
        let dispatcher = CapabilityDispatcher::new(config.clone());
        Self {
            config,
            session,
            ws_url,
            connection_token,
            dispatcher,
        }
    }

    pub async fn run_forever(&self) -> Result<()> {
        let mut backoff = Duration::from_secs(1);
        loop {
            match self.run_once().await {
                Ok(()) => {
                    backoff = Duration::from_secs(1);
                }
                Err(error) => {
                    warn!(?error, "websocket disconnected");
                    time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(60));
                }
            }
        }
    }

    async fn run_once(&self) -> Result<()> {
        let url = self.ws_connect_url();
        info!(url = %url, "connecting websocket");
        let mut request = url.into_client_request().context("invalid websocket url")?;
        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.session.token))
                .context("invalid authorization header")?,
        );
        request.headers_mut().insert(
            "X-Device-Id",
            HeaderValue::from_str(&self.config.client.device_id)?,
        );
        request.headers_mut().insert(
            "X-Client-Version",
            HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
        );
        request
            .headers_mut()
            .insert("X-Protocol-Version", HeaderValue::from_static("1.0"));

        let (stream, _) = connect_async(request)
            .await
            .context("failed to connect websocket")?;
        info!("websocket connected");
        let (mut write, mut read) = stream.split();

        let register_payload =
            DeviceRegisterRequest::from_config(&self.config, self.session.user.clone());
        send_json(
            &mut write,
            &Envelope::new("client.register", serde_json::to_value(register_payload)?),
        )
        .await?;

        let mut session_id: Option<String> = None;
        let mut heartbeat = time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    send_json(&mut write, &client_ping(session_id.as_deref())).await?;
                }
                message = read.next() => {
                    let Some(message) = message else {
                        bail!("websocket stream ended");
                    };
                    match message.context("failed to read websocket message")? {
                        Message::Text(text) => {
                            self.handle_text(&mut write, &text, &mut session_id).await?;
                        }
                        Message::Close(frame) => {
                            bail!("websocket closed: {frame:?}");
                        }
                        Message::Ping(bytes) => {
                            write.send(Message::Pong(bytes)).await?;
                        }
                        Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                    }
                }
            }
        }
    }

    async fn handle_text<S>(
        &self,
        write: &mut S,
        text: &str,
        session_id: &mut Option<String>,
    ) -> Result<()>
    where
        S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    {
        let envelope: Envelope<Value> =
            serde_json::from_str(text).context("failed to parse websocket envelope")?;

        match envelope.message_type.as_str() {
            "server.registered" => {
                *session_id = envelope
                    .payload
                    .get("session_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                info!(?session_id, "client registered");
            }
            "server.pong" => {}
            "task.request" => {
                let task: TaskRequestPayload =
                    match serde_json::from_value(envelope.payload.clone()) {
                        Ok(task) => task,
                        Err(source) => {
                            let error =
                                anyhow::Error::new(source).context("invalid task.request payload");
                            let task_id = envelope
                                .payload
                                .get("task_id")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown");
                            send_json(
                                write,
                                &task_rejected(
                                    &envelope.id,
                                    task_id,
                                    error_code(&error),
                                    error_message(&error),
                                    json!({ "causes": error_causes(&error) }),
                                ),
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                send_json(
                    write,
                    &client_ack(&envelope.id, Some(&task.task_id), "received"),
                )
                .await?;
                send_json(write, &task_accepted(&envelope.id, &task.task_id)).await?;

                match self
                    .dispatcher
                    .dispatch(&task.capability, task.params)
                    .await
                {
                    Ok(output) => {
                        send_json(
                            write,
                            &task_result(&task.task_id, output.result, output.duration_ms),
                        )
                        .await?;
                    }
                    Err(error) => {
                        error!(?error, task_id = task.task_id, "task failed");
                        send_json(
                            write,
                            &task_failed(
                                &task.task_id,
                                error_code(&error),
                                error_message(&error),
                                json!({
                                    "capability": task.capability,
                                    "causes": error_causes(&error)
                                }),
                            ),
                        )
                        .await?;
                    }
                }
            }
            "task.cancel" => {
                let task_id = envelope
                    .payload
                    .get("task_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                send_json(
                    write,
                    &Envelope::new(
                        "task.cancelled",
                        json!({
                            "task_id": task_id,
                            "status": "cancelled"
                        }),
                    )
                    .reply_to(envelope.id),
                )
                .await?;
            }
            "server.reauth_required" => bail!("server requested reauthentication"),
            "server.disconnect" => bail!("server requested disconnect: {}", envelope.payload),
            "server.policy_updated" => {
                warn!(
                    "server policy update received; dynamic policy refresh is not implemented yet"
                );
            }
            other => {
                warn!(message_type = other, "unsupported websocket message");
            }
        }
        Ok(())
    }

    fn ws_connect_url(&self) -> String {
        if let Some(token) = &self.connection_token {
            let separator = if self.ws_url.contains('?') { '&' } else { '?' };
            format!("{}{}connection_token={}", self.ws_url, separator, token)
        } else {
            self.ws_url.clone()
        }
    }
}

fn resolve_ws_url(config: &AppConfig, candidate: &str) -> String {
    let candidate = candidate.trim();
    if candidate.contains("://") {
        return candidate.to_string();
    }

    if candidate.starts_with('/') {
        if let Ok(mut base) = Url::parse(&config.server.ws_url) {
            base.set_path(candidate);
            base.set_query(None);
            return base.to_string();
        }

        if let Ok(mut base) = Url::parse(&config.server.api_base_url) {
            let scheme = match base.scheme() {
                "https" => "wss",
                _ => "ws",
            };
            let _ = base.set_scheme(scheme);
            base.set_path(candidate);
            base.set_query(None);
            return base.to_string();
        }
    }

    candidate.to_string()
}

pub async fn send_goodbye(config: AppConfig, session: Session, reason: &str) -> Result<()> {
    let client = AgentWsClient::new(config, session, None, None);
    let url = client.ws_connect_url();
    let mut request = url.into_client_request().context("invalid websocket url")?;
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", client.session.token))?,
    );
    let (mut stream, _) = connect_async(request).await?;
    send_json(&mut stream, &client_goodbye(None, reason)).await
}

async fn send_json<S>(write: &mut S, envelope: &Envelope<Value>) -> Result<()>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let raw = serde_json::to_string(envelope).context("failed to serialize websocket message")?;
    write.send(Message::Text(raw.into())).await?;
    Ok(())
}
