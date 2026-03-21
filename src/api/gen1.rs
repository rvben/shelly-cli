use anyhow::{Context, Result};

use crate::model::{DeviceInfo, DeviceStatus, PowerReading, SwitchStatus};

use super::{FirmwareInfo, SwitchResult};

pub struct Gen1Device {
    info: DeviceInfo,
    client: reqwest::Client,
    password: Option<String>,
}

impl Gen1Device {
    pub fn new(info: DeviceInfo, client: reqwest::Client, password: Option<String>) -> Self {
        Self {
            info,
            client,
            password,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.info.ip)
    }

    async fn get_json(&self, path: &str) -> Result<serde_json::Value> {
        let url = self.url(path);
        let mut req = self.client.get(&url);
        if let Some(ref password) = self.password {
            req = req.basic_auth("admin", Some(password));
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("failed to reach {url}"))?;

        if !resp.status().is_success() {
            anyhow::bail!("HTTP {} from {url}", resp.status());
        }

        resp.json()
            .await
            .with_context(|| format!("invalid JSON from {url}"))
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    pub async fn status(&self) -> Result<DeviceStatus> {
        let status = self.get_json("/status").await?;
        Ok(DeviceStatus::from_gen1(&status))
    }

    pub async fn switch_status(&self, id: u8) -> Result<SwitchStatus> {
        let status = self.get_json("/status").await?;

        let relays = status
            .get("relays")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("no relays in status"))?;

        let relay = relays
            .get(id as usize)
            .ok_or_else(|| anyhow::anyhow!("relay {id} not found"))?;

        let meter = status
            .get("meters")
            .and_then(|v| v.as_array())
            .and_then(|m| m.get(id as usize));

        Ok(SwitchStatus::from_gen1_relay_json(id, relay, meter))
    }

    pub async fn switch_set(&self, id: u8, on: bool) -> Result<SwitchResult> {
        let turn = if on { "on" } else { "off" };
        let resp = self.get_json(&format!("/relay/{id}?turn={turn}")).await?;

        let was_on = resp.get("ison").and_then(|v| v.as_bool()).unwrap_or(false);

        Ok(SwitchResult { was_on })
    }

    pub async fn switch_toggle(&self, id: u8) -> Result<SwitchResult> {
        let resp = self.get_json(&format!("/relay/{id}?turn=toggle")).await?;

        let was_on = resp.get("ison").and_then(|v| v.as_bool()).unwrap_or(false);

        Ok(SwitchResult { was_on })
    }

    pub async fn power(&self, id: u8) -> Result<PowerReading> {
        let status = self.get_json("/status").await?;

        let meter = status
            .get("meters")
            .and_then(|v| v.as_array())
            .and_then(|m| m.get(id as usize))
            .ok_or_else(|| anyhow::anyhow!("meter {id} not found"))?;

        let power = meter.get("power").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let total = meter.get("total").and_then(|v| v.as_f64()).unwrap_or(0.0);

        let voltage = status.get("voltage").and_then(|v| v.as_f64());

        Ok(PowerReading {
            id,
            power_watts: power,
            voltage,
            current: None,
            total_energy_wh: total,
        })
    }

    pub async fn firmware_check(&self) -> Result<FirmwareInfo> {
        let status = self.get_json("/status").await?;

        let update = status
            .get("update")
            .ok_or_else(|| anyhow::anyhow!("no update info in status"))?;

        let has_update = update
            .get("has_update")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let current = update
            .get("old_version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let stable = update
            .get("new_version")
            .and_then(|v| v.as_str())
            .map(String::from);

        let beta = update
            .get("beta_version")
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(FirmwareInfo {
            current_version: current,
            has_update,
            stable_version: stable,
            beta_version: beta,
        })
    }

    pub async fn config_get(&self) -> Result<serde_json::Value> {
        self.get_json("/settings").await
    }

    pub async fn reboot(&self) -> Result<()> {
        self.get_json("/reboot").await?;
        Ok(())
    }

    pub async fn firmware_update(&self) -> Result<()> {
        self.get_json("/ota?update=true").await?;
        Ok(())
    }

    pub async fn config_set(&self, key: &str, value: &str) -> Result<()> {
        let param = match key {
            "name" => ("name", value.to_string()),
            "eco_mode" => ("eco_mode_enabled", value.to_string()),
            "led_status_disable" => ("led_status_disable", value.to_string()),
            _ => {
                anyhow::bail!(
                    "unknown config key '{key}'. Supported keys: name, eco_mode, led_status_disable"
                );
            }
        };
        self.get_json(&format!("/settings?{}={}", param.0, param.1))
            .await?;
        Ok(())
    }

    pub async fn schedule_list(&self) -> Result<serde_json::Value> {
        anyhow::bail!("schedules are not supported on Gen1 devices")
    }

    pub async fn webhook_list(&self) -> Result<serde_json::Value> {
        let settings = self.get_json("/settings").await?;
        Ok(settings
            .get("actions")
            .cloned()
            .unwrap_or(serde_json::json!({})))
    }

    pub async fn set_name(&self, name: &str) -> Result<()> {
        let url = self.url("/settings");
        let mut req = self.client.get(&url).query(&[("name", name)]);
        if let Some(ref password) = self.password {
            req = req.basic_auth("admin", Some(password));
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("failed to reach {url}"))?;

        if !resp.status().is_success() {
            anyhow::bail!("HTTP {} from {url}", resp.status());
        }
        Ok(())
    }
}
