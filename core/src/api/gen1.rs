use crate::Result;
use crate::error::{self, Error};
use crate::model::{DeviceInfo, DeviceStatus, PowerReading, SwitchStatus};

use super::{FirmwareInfo, SwitchResult};

pub struct Gen1Device {
    info: DeviceInfo,
    base_host: String,
    client: reqwest::Client,
    password: Option<String>,
}

impl Gen1Device {
    pub fn new(info: DeviceInfo, client: reqwest::Client, password: Option<String>) -> Self {
        let base_host = info.ip.to_string();
        Self::new_with_host(info, base_host, client, password)
    }

    /// Build a `Gen1Device` addressed by an explicit `host[:port]` string
    /// rather than `info.ip`, so a device that isn't reachable on the
    /// default port (or that must be reached by a test harness on an
    /// ephemeral loopback port) can still be targeted.
    pub fn new_with_host(
        info: DeviceInfo,
        base_host: String,
        client: reqwest::Client,
        password: Option<String>,
    ) -> Self {
        Self {
            info,
            base_host,
            client,
            password,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.base_host)
    }

    async fn get_json(&self, path: &str) -> Result<serde_json::Value> {
        let url = self.url(path);
        let mut req = self.client.get(&url);
        if let Some(ref password) = self.password {
            req = req.basic_auth("admin", Some(password));
        }
        let resp = req.send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error::status_error(status, &url, &body));
        }

        Ok(resp.json().await?)
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
            .ok_or_else(|| Error::Parse {
                message: "no relays in status".to_string(),
            })?;

        let relay = relays.get(id as usize).ok_or_else(|| Error::Parse {
            message: format!("relay {id} not found"),
        })?;

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
            .ok_or_else(|| Error::Parse {
                message: format!("meter {id} not found"),
            })?;

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

        let update = status.get("update").ok_or_else(|| Error::Parse {
            message: "no update info in status".to_string(),
        })?;

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
                return Err(Error::Unsupported {
                    message: format!(
                        "unknown config key '{key}'. Supported keys: name, eco_mode, led_status_disable"
                    ),
                });
            }
        };
        self.get_json(&format!("/settings?{}={}", param.0, param.1))
            .await?;
        Ok(())
    }

    pub async fn schedule_list(&self) -> Result<serde_json::Value> {
        Err(Error::Unsupported {
            message: "schedules are not supported on Gen1 devices".to_string(),
        })
    }

    pub async fn webhook_list(&self) -> Result<serde_json::Value> {
        let settings = self.get_json("/settings").await?;
        Ok(settings
            .get("actions")
            .cloned()
            .unwrap_or(serde_json::json!({})))
    }

    pub async fn config_restore(&self, config: &serde_json::Value) -> Result<()> {
        // For Gen1, restore only safe top-level settings
        // Skip network/WiFi/MQTT/cloud settings
        const SKIP_KEYS: &[&str] = &[
            "wifi_ap",
            "wifi_sta",
            "wifi_sta1",
            "mqtt",
            "coiot",
            "sntp",
            "login",
            "ap_roaming",
        ];

        let obj = config.as_object().ok_or_else(|| Error::Parse {
            message: "config must be a JSON object".to_string(),
        })?;

        let mut params = Vec::new();
        for (key, value) in obj {
            if SKIP_KEYS.contains(&key.as_str()) {
                continue;
            }

            // Only restore simple scalar values via /settings?key=value
            match value {
                serde_json::Value::String(s) => params.push(format!("{key}={s}")),
                serde_json::Value::Bool(b) => params.push(format!("{key}={b}")),
                serde_json::Value::Number(n) => params.push(format!("{key}={n}")),
                _ => continue,
            }
        }

        if !params.is_empty() {
            let query = params.join("&");
            self.get_json(&format!("/settings?{query}")).await?;
        }

        Ok(())
    }

    pub async fn set_name(&self, name: &str) -> Result<()> {
        let url = self.url("/settings");
        let mut req = self.client.get(&url).query(&[("name", name)]);
        if let Some(ref password) = self.password {
            req = req.basic_auth("admin", Some(password));
        }
        let resp = req.send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error::status_error(status, &url, &body));
        }
        Ok(())
    }
}
