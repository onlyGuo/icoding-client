use crate::{config::AppConfig, device};
use anyhow::Result;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct SystemCapabilities {
    config: AppConfig,
}

impl SystemCapabilities {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub fn info(&self) -> Result<Value> {
        Ok(json!({
            "client_version": env!("CARGO_PKG_VERSION"),
            "protocol_version": crate::protocol::PROTOCOL_VERSION,
            "device_id": self.config.client.device_id,
            "system": device::collect_system_info(),
            "capabilities": device::default_capabilities()
        }))
    }
}
