use anyhow::{Context, Result};

use crate::model::{DeviceInfo, DeviceStatus, PowerReading, SwitchStatus};

use super::{FirmwareInfo, SwitchResult};

pub struct Gen2Device {
    info: DeviceInfo,
    client: reqwest::Client,
    password: Option<String>,
}

impl Gen2Device {
    pub fn new(info: DeviceInfo, client: reqwest::Client, password: Option<String>) -> Self {
        Self {
            info,
            client,
            password,
        }
    }

    fn rpc_url(&self, method: &str) -> String {
        format!("http://{}/rpc/{method}", self.info.ip)
    }

    async fn rpc_call(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let url = self.rpc_url(method);

        let resp = if let Some(params) = params {
            let mut req = self.client.post(&url).json(&params);
            if let Some(ref password) = self.password {
                req = req.basic_auth("admin", Some(password));
            }
            req.send().await
        } else {
            let mut req = self.client.get(&url);
            if let Some(ref password) = self.password {
                req = req.basic_auth("admin", Some(password));
            }
            req.send().await
        };

        let resp = resp.with_context(|| format!("failed to reach {url}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {status} from {url}: {body}");
        }

        resp.json()
            .await
            .with_context(|| format!("invalid JSON from {url}"))
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    pub async fn status(&self) -> Result<DeviceStatus> {
        let status = self.rpc_call("Shelly.GetStatus", None).await?;
        Ok(DeviceStatus::from_gen2(&status))
    }

    pub async fn switch_status(&self, id: u8) -> Result<SwitchStatus> {
        let params = serde_json::json!({ "id": id });
        let resp = self.rpc_call("Switch.GetStatus", Some(params)).await?;
        Ok(SwitchStatus::from_gen2_switch_json(&resp))
    }

    pub async fn switch_set(&self, id: u8, on: bool) -> Result<SwitchResult> {
        let params = serde_json::json!({ "id": id, "on": on });
        let resp = self.rpc_call("Switch.Set", Some(params)).await?;

        let was_on = resp
            .get("was_on")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(SwitchResult { was_on })
    }

    pub async fn switch_toggle(&self, id: u8) -> Result<SwitchResult> {
        let params = serde_json::json!({ "id": id });
        let resp = self.rpc_call("Switch.Toggle", Some(params)).await?;

        let was_on = resp
            .get("was_on")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(SwitchResult { was_on })
    }

    pub async fn power(&self, id: u8) -> Result<PowerReading> {
        let params = serde_json::json!({ "id": id });
        let resp = self.rpc_call("Switch.GetStatus", Some(params)).await?;

        let power = resp.get("apower").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let voltage = resp.get("voltage").and_then(|v| v.as_f64());
        let current = resp.get("current").and_then(|v| v.as_f64());
        let total = resp
            .get("aenergy")
            .and_then(|v| v.get("total"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        Ok(PowerReading {
            id,
            power_watts: power,
            voltage,
            current,
            total_energy_wh: total,
        })
    }

    pub async fn firmware_check(&self) -> Result<FirmwareInfo> {
        let resp = self.rpc_call("Shelly.CheckForUpdate", None).await?;
        let dev_info = self.rpc_call("Shelly.GetDeviceInfo", None).await?;

        let current = dev_info
            .get("ver")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let stable = resp
            .get("stable")
            .and_then(|v| v.get("version"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let beta = resp
            .get("beta")
            .and_then(|v| v.get("version"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let has_update = stable.is_some();

        Ok(FirmwareInfo {
            current_version: current,
            has_update,
            stable_version: stable,
            beta_version: beta,
        })
    }

    pub async fn config_get(&self) -> Result<serde_json::Value> {
        self.rpc_call("Shelly.GetConfig", None).await
    }

    pub async fn reboot(&self) -> Result<()> {
        self.rpc_call("Shelly.Reboot", None).await?;
        Ok(())
    }

    pub async fn firmware_update(&self) -> Result<()> {
        let params = serde_json::json!({ "stage": "stable" });
        self.rpc_call("Shelly.Update", Some(params)).await?;
        Ok(())
    }

    pub async fn config_set(&self, key: &str, value: &str) -> Result<()> {
        // Map user-friendly keys to Gen2 RPC config paths
        let (component, config_key) = match key {
            "name" => ("sys", "device"),
            "eco_mode" => ("sys", "device"),
            "led_status_disable" | "led" => ("sys", "ui"),
            _ => {
                anyhow::bail!(
                    "unknown config key '{key}'. Supported keys: name, eco_mode, led_status_disable"
                );
            }
        };

        let parsed_value: serde_json::Value = match value {
            "true" => serde_json::Value::Bool(true),
            "false" => serde_json::Value::Bool(false),
            v if v.parse::<f64>().is_ok() => {
                serde_json::Value::Number(serde_json::Number::from_f64(v.parse().unwrap()).unwrap())
            }
            v => serde_json::Value::String(v.to_string()),
        };

        let config = match key {
            "name" => serde_json::json!({ component: { config_key: { "name": parsed_value } } }),
            "eco_mode" => {
                serde_json::json!({ component: { config_key: { "eco_mode": parsed_value } } })
            }
            "led_status_disable" | "led" => {
                // Gen3 Mini uses sys.ui, but not all devices support it
                serde_json::json!({ component: { config_key: { "led_status_disable": parsed_value } } })
            }
            _ => unreachable!(),
        };

        self.rpc_call(
            "Sys.SetConfig",
            Some(serde_json::json!({ "config": config[component] })),
        )
        .await?;
        Ok(())
    }

    pub async fn schedule_list(&self) -> Result<serde_json::Value> {
        let resp = self.rpc_call("Schedule.List", None).await?;
        Ok(resp
            .get("jobs")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![])))
    }

    pub async fn webhook_list(&self) -> Result<serde_json::Value> {
        let resp = self.rpc_call("Webhook.List", None).await?;
        Ok(resp
            .get("hooks")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![])))
    }

    pub async fn set_name(&self, name: &str) -> Result<()> {
        let params = serde_json::json!({
            "config": { "device": { "name": name } }
        });
        self.rpc_call("Sys.SetConfig", Some(params)).await?;
        Ok(())
    }
}
