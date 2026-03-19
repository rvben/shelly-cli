pub mod discovery;
pub mod gen1;
pub mod gen2;

use std::net::IpAddr;

use anyhow::Result;
use async_trait::async_trait;

use crate::model::{DeviceInfo, DeviceStatus, PowerReading, SwitchStatus};

#[derive(Debug, Clone)]
pub struct SwitchResult {
    pub was_on: bool,
}

#[derive(Debug, Clone)]
pub struct FirmwareInfo {
    pub current_version: String,
    pub has_update: bool,
    pub stable_version: Option<String>,
    pub beta_version: Option<String>,
}

#[async_trait]
pub trait ShellyDevice: Send + Sync {
    fn info(&self) -> &DeviceInfo;

    async fn status(&self) -> Result<DeviceStatus>;

    async fn switch_status(&self, id: u8) -> Result<SwitchStatus>;

    async fn switch_set(&self, id: u8, on: bool) -> Result<SwitchResult>;

    async fn switch_toggle(&self, id: u8) -> Result<SwitchResult>;

    async fn power(&self, id: u8) -> Result<PowerReading>;

    async fn firmware_check(&self) -> Result<FirmwareInfo>;

    async fn config_get(&self) -> Result<serde_json::Value>;

    async fn reboot(&self) -> Result<()>;
}

pub fn create_device(
    info: DeviceInfo,
    client: reqwest::Client,
) -> Box<dyn ShellyDevice> {
    match info.generation {
        crate::model::DeviceGeneration::Gen1 => {
            Box::new(gen1::Gen1Device::new(info, client))
        }
        crate::model::DeviceGeneration::Gen2 | crate::model::DeviceGeneration::Gen3 => {
            Box::new(gen2::Gen2Device::new(info, client))
        }
    }
}

pub async fn probe_device(ip: IpAddr, client: &reqwest::Client) -> Result<DeviceInfo> {
    let url = format!("http://{ip}/shelly");
    let resp = client.get(&url).send().await?;
    let shelly: serde_json::Value = resp.json().await?;
    DeviceInfo::from_shelly_response(ip, &shelly)
        .ok_or_else(|| anyhow::anyhow!("unrecognized Shelly response from {ip}"))
}
