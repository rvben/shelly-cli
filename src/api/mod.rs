pub mod discovery;
pub mod gen1;
pub mod gen2;

use std::net::IpAddr;

use anyhow::Result;

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

pub enum ShellyDevice {
    Gen1(gen1::Gen1Device),
    Gen2(gen2::Gen2Device),
}

impl ShellyDevice {
    pub fn info(&self) -> &DeviceInfo {
        match self {
            Self::Gen1(d) => d.info(),
            Self::Gen2(d) => d.info(),
        }
    }

    pub async fn status(&self) -> Result<DeviceStatus> {
        match self {
            Self::Gen1(d) => d.status().await,
            Self::Gen2(d) => d.status().await,
        }
    }

    pub async fn switch_status(&self, id: u8) -> Result<SwitchStatus> {
        match self {
            Self::Gen1(d) => d.switch_status(id).await,
            Self::Gen2(d) => d.switch_status(id).await,
        }
    }

    pub async fn switch_set(&self, id: u8, on: bool) -> Result<SwitchResult> {
        match self {
            Self::Gen1(d) => d.switch_set(id, on).await,
            Self::Gen2(d) => d.switch_set(id, on).await,
        }
    }

    pub async fn switch_toggle(&self, id: u8) -> Result<SwitchResult> {
        match self {
            Self::Gen1(d) => d.switch_toggle(id).await,
            Self::Gen2(d) => d.switch_toggle(id).await,
        }
    }

    pub async fn power(&self, id: u8) -> Result<PowerReading> {
        match self {
            Self::Gen1(d) => d.power(id).await,
            Self::Gen2(d) => d.power(id).await,
        }
    }

    pub async fn firmware_check(&self) -> Result<FirmwareInfo> {
        match self {
            Self::Gen1(d) => d.firmware_check().await,
            Self::Gen2(d) => d.firmware_check().await,
        }
    }

    pub async fn config_get(&self) -> Result<serde_json::Value> {
        match self {
            Self::Gen1(d) => d.config_get().await,
            Self::Gen2(d) => d.config_get().await,
        }
    }

    pub async fn reboot(&self) -> Result<()> {
        match self {
            Self::Gen1(d) => d.reboot().await,
            Self::Gen2(d) => d.reboot().await,
        }
    }

    pub async fn firmware_update(&self) -> Result<()> {
        match self {
            Self::Gen1(d) => d.firmware_update().await,
            Self::Gen2(d) => d.firmware_update().await,
        }
    }

    pub async fn config_set(&self, key: &str, value: &str) -> Result<()> {
        match self {
            Self::Gen1(d) => d.config_set(key, value).await,
            Self::Gen2(d) => d.config_set(key, value).await,
        }
    }

    pub async fn schedule_list(&self) -> Result<serde_json::Value> {
        match self {
            Self::Gen1(d) => d.schedule_list().await,
            Self::Gen2(d) => d.schedule_list().await,
        }
    }

    pub async fn webhook_list(&self) -> Result<serde_json::Value> {
        match self {
            Self::Gen1(d) => d.webhook_list().await,
            Self::Gen2(d) => d.webhook_list().await,
        }
    }

    pub async fn set_name(&self, name: &str) -> Result<()> {
        match self {
            Self::Gen1(d) => d.set_name(name).await,
            Self::Gen2(d) => d.set_name(name).await,
        }
    }
}

pub fn create_device(
    info: DeviceInfo,
    client: reqwest::Client,
    password: Option<String>,
) -> ShellyDevice {
    match info.generation {
        crate::model::DeviceGeneration::Gen1 => {
            ShellyDevice::Gen1(gen1::Gen1Device::new(info, client, password))
        }
        crate::model::DeviceGeneration::Gen2 | crate::model::DeviceGeneration::Gen3 => {
            ShellyDevice::Gen2(gen2::Gen2Device::new(info, client, password))
        }
    }
}

pub async fn probe_device(ip: IpAddr, client: &reqwest::Client) -> Result<DeviceInfo> {
    let url = format!("http://{ip}/shelly");
    let resp = client.get(&url).send().await?;
    let shelly: serde_json::Value = resp.json().await?;
    let mut info = DeviceInfo::from_shelly_response(ip, &shelly)
        .ok_or_else(|| anyhow::anyhow!("unrecognized Shelly response from {ip}"))?;

    // Gen2/Gen3 devices don't report num_outputs in /shelly, so count switch
    // components from the full status response.
    if matches!(
        info.generation,
        crate::model::DeviceGeneration::Gen2 | crate::model::DeviceGeneration::Gen3
    ) && let Ok((num_outputs, num_meters)) = count_gen2_outputs(ip, client).await
    {
        info.num_outputs = num_outputs;
        info.num_meters = num_meters;
    }

    Ok(info)
}

/// Count switch components from a Gen2/Gen3 `Shelly.GetStatus` response.
async fn count_gen2_outputs(ip: IpAddr, client: &reqwest::Client) -> Result<(u8, u8)> {
    let url = format!("http://{ip}/rpc/Shelly.GetStatus");
    let resp = client.get(&url).send().await?;
    let status: serde_json::Value = resp.json().await?;

    let obj = status
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("expected JSON object"))?;

    let num_switches = obj.keys().filter(|k| k.starts_with("switch:")).count() as u8;

    // Gen2 power metering is embedded in each switch component
    Ok((num_switches.max(1), num_switches.max(1)))
}
