use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::IpAddr;

use super::gen1_responses::Gen1ShellyResponse;
use super::gen2_responses::Gen2ShellyResponse;

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
        let resp: Gen1ShellyResponse = serde_json::from_value(shelly.clone()).ok()?;

        let hostname = format!(
            "shelly{}-{}",
            resp.device_type.to_lowercase().replace("shsw-", ""),
            resp.mac
        );

        Some(Self {
            ip,
            name: None,
            id: hostname,
            mac: resp.mac,
            model: resp.device_type.clone(),
            generation: DeviceGeneration::Gen1,
            firmware_version: resp.fw,
            auth_enabled: resp.auth,
            num_outputs: resp.num_outputs,
            num_meters: resp.num_meters,
            app: None,
            device_type: Some(resp.device_type),
        })
    }

    pub fn from_gen2_shelly(ip: IpAddr, shelly: &serde_json::Value) -> Option<Self> {
        let resp: Gen2ShellyResponse = serde_json::from_value(shelly.clone()).ok()?;

        let generation = if resp.generation >= 3 {
            DeviceGeneration::Gen3
        } else {
            DeviceGeneration::Gen2
        };

        Some(Self {
            ip,
            name: resp.name,
            id: resp.id,
            mac: resp.mac,
            model: resp.model,
            generation,
            firmware_version: resp.ver,
            auth_enabled: resp.auth_en,
            num_outputs: 1,
            num_meters: 1,
            app: resp.app,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ip() -> IpAddr {
        "10.10.20.1".parse().unwrap()
    }

    #[test]
    fn gen1_device_from_shelly_response() {
        let shelly = json!({
            "type": "SHSW-PM",
            "mac": "AABBCCDDEEFF",
            "auth": false,
            "fw": "20230913-114003/v1.14.0-gcb84623",
            "num_outputs": 1,
            "num_meters": 1
        });

        let device = DeviceInfo::from_shelly_response(ip(), &shelly).unwrap();
        assert_eq!(device.generation, DeviceGeneration::Gen1);
        assert_eq!(device.model, "SHSW-PM");
        assert_eq!(device.mac, "AABBCCDDEEFF");
        assert_eq!(device.firmware_version, "20230913-114003/v1.14.0-gcb84623");
        assert!(!device.auth_enabled);
        assert_eq!(device.num_outputs, 1);
        assert_eq!(device.num_meters, 1);
        assert_eq!(device.id, "shellypm-AABBCCDDEEFF");
        assert!(device.name.is_none());
        assert_eq!(device.device_type.as_deref(), Some("SHSW-PM"));
        assert!(device.app.is_none());
    }

    #[test]
    fn gen2_device_from_shelly_response() {
        let shelly = json!({
            "id": "shellyplus1pm-aabbccddeeff",
            "mac": "AABBCCDDEEFF",
            "model": "SNSW-001P16EU",
            "gen": 2,
            "fw_id": "20230913-114003",
            "ver": "1.0.0",
            "app": "Plus1PM",
            "auth_en": false,
            "name": "Living Room"
        });

        let device = DeviceInfo::from_shelly_response(ip(), &shelly).unwrap();
        assert_eq!(device.generation, DeviceGeneration::Gen2);
        assert_eq!(device.model, "SNSW-001P16EU");
        assert_eq!(device.mac, "AABBCCDDEEFF");
        assert_eq!(device.id, "shellyplus1pm-aabbccddeeff");
        assert_eq!(device.name.as_deref(), Some("Living Room"));
        assert_eq!(device.app.as_deref(), Some("Plus1PM"));
        assert!(!device.auth_enabled);
        assert_eq!(device.firmware_version, "1.0.0");
    }

    #[test]
    fn gen3_device_from_shelly_response() {
        let shelly = json!({
            "id": "shelly1minig3-aabbccddeeff",
            "mac": "AABBCCDDEEFF",
            "model": "S3SW-001X8EU",
            "gen": 3,
            "fw_id": "20240101-000000",
            "ver": "2.0.0",
            "app": "Mini1G3",
            "auth_en": true,
            "name": null
        });

        let device = DeviceInfo::from_shelly_response(ip(), &shelly).unwrap();
        assert_eq!(device.generation, DeviceGeneration::Gen3);
        assert_eq!(device.model, "S3SW-001X8EU");
        assert!(device.auth_enabled);
        assert!(device.name.is_none());
        assert_eq!(device.app.as_deref(), Some("Mini1G3"));
    }

    #[test]
    fn missing_optional_fields_gen1() {
        let shelly = json!({
            "type": "SHSW-1",
            "mac": "112233445566"
        });

        let device = DeviceInfo::from_shelly_response(ip(), &shelly).unwrap();
        assert_eq!(device.generation, DeviceGeneration::Gen1);
        assert!(!device.auth_enabled);
        assert_eq!(device.firmware_version, "unknown");
        assert_eq!(device.num_outputs, 1);
        assert_eq!(device.num_meters, 0);
    }

    #[test]
    fn missing_optional_fields_gen2() {
        let shelly = json!({
            "id": "shellyplus1-abc",
            "mac": "112233445566",
            "gen": 2
        });

        let device = DeviceInfo::from_shelly_response(ip(), &shelly).unwrap();
        assert_eq!(device.generation, DeviceGeneration::Gen2);
        assert_eq!(device.model, "unknown");
        assert_eq!(device.firmware_version, "unknown");
        assert!(device.name.is_none());
        assert!(device.app.is_none());
    }

    #[test]
    fn unknown_extra_fields_ignored() {
        let shelly = json!({
            "type": "SHSW-PM",
            "mac": "AABBCCDDEEFF",
            "auth": false,
            "fw": "1.0.0",
            "num_outputs": 1,
            "num_meters": 1,
            "totally_new_field": "should be ignored",
            "another_field": 42
        });

        let device = DeviceInfo::from_shelly_response(ip(), &shelly).unwrap();
        assert_eq!(device.model, "SHSW-PM");
    }

    #[test]
    fn unrecognized_response_returns_none() {
        let shelly = json!({"random": "data"});
        assert!(DeviceInfo::from_shelly_response(ip(), &shelly).is_none());
    }

    #[test]
    fn display_name_uses_name_when_present() {
        let shelly = json!({
            "id": "shellyplus1pm-abc",
            "mac": "AABBCCDDEEFF",
            "gen": 2,
            "ver": "1.0",
            "name": "Kitchen Light"
        });
        let device = DeviceInfo::from_shelly_response(ip(), &shelly).unwrap();
        assert_eq!(device.display_name(), "Kitchen Light");
    }

    #[test]
    fn display_name_falls_back_to_id() {
        let shelly = json!({
            "type": "SHSW-PM",
            "mac": "AABBCCDDEEFF",
            "fw": "1.0"
        });
        let device = DeviceInfo::from_shelly_response(ip(), &shelly).unwrap();
        assert_eq!(device.display_name(), "shellypm-AABBCCDDEEFF");
    }

    #[test]
    fn gen1_hostname_strips_shsw_prefix() {
        let shelly = json!({
            "type": "SHSW-25",
            "mac": "AABBCCDDEEFF",
            "fw": "1.0"
        });
        let device = DeviceInfo::from_gen1_shelly(ip(), &shelly).unwrap();
        assert_eq!(device.id, "shelly25-AABBCCDDEEFF");
    }
}
