use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::IpAddr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceGeneration {
    Gen1,
    Gen2,
    Gen3,
}

impl fmt::Display for DeviceGeneration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gen1 => write!(f, "Gen1"),
            Self::Gen2 => write!(f, "Gen2"),
            Self::Gen3 => write!(f, "Gen3"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub ip: IpAddr,
    pub name: Option<String>,
    pub id: String,
    pub mac: String,
    pub model: String,
    pub generation: DeviceGeneration,
    pub firmware_version: String,
    pub auth_enabled: bool,
    pub num_outputs: u8,
    pub num_meters: u8,
    pub app: Option<String>,
    pub device_type: Option<String>,
}

impl DeviceInfo {
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }

    pub fn from_gen1_shelly(ip: IpAddr, shelly: &serde_json::Value) -> Option<Self> {
        let device_type = shelly.get("type")?.as_str()?;
        let mac = shelly.get("mac")?.as_str()?;
        let fw = shelly.get("fw")?.as_str().unwrap_or("unknown");
        let auth = shelly.get("auth").and_then(|v| v.as_bool()).unwrap_or(false);
        let num_outputs = shelly
            .get("num_outputs")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u8;
        let num_meters = shelly
            .get("num_meters")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u8;

        let hostname = format!(
            "shelly{}-{}",
            device_type.to_lowercase().replace("shsw-", ""),
            mac
        );

        Some(Self {
            ip,
            name: None,
            id: hostname,
            mac: mac.to_string(),
            model: device_type.to_string(),
            generation: DeviceGeneration::Gen1,
            firmware_version: fw.to_string(),
            auth_enabled: auth,
            num_outputs,
            num_meters,
            app: None,
            device_type: Some(device_type.to_string()),
        })
    }

    pub fn from_gen2_shelly(ip: IpAddr, shelly: &serde_json::Value) -> Option<Self> {
        let id = shelly.get("id")?.as_str()?;
        let mac = shelly.get("mac")?.as_str()?;
        let model = shelly.get("model")?.as_str().unwrap_or("unknown");
        let generation_num = shelly.get("gen").and_then(|v| v.as_u64()).unwrap_or(2);
        let ver = shelly.get("ver")?.as_str().unwrap_or("unknown");
        let auth = shelly
            .get("auth_en")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let app = shelly.get("app").and_then(|v| v.as_str()).map(String::from);
        let name = shelly
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from);

        let generation = if generation_num >= 3 {
            DeviceGeneration::Gen3
        } else {
            DeviceGeneration::Gen2
        };

        Some(Self {
            ip,
            name,
            id: id.to_string(),
            mac: mac.to_string(),
            model: model.to_string(),
            generation,
            firmware_version: ver.to_string(),
            auth_enabled: auth,
            num_outputs: 1,
            num_meters: 1,
            app,
            device_type: None,
        })
    }

    pub fn from_shelly_response(ip: IpAddr, shelly: &serde_json::Value) -> Option<Self> {
        if shelly.get("gen").is_some() {
            Self::from_gen2_shelly(ip, shelly)
        } else if shelly.get("type").is_some() {
            Self::from_gen1_shelly(ip, shelly)
        } else {
            None
        }
    }
}
