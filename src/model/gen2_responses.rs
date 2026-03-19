use serde::Deserialize;

/// Gen2/Gen3 `/shelly` endpoint response.
#[derive(Debug, Deserialize)]
pub struct Gen2ShellyResponse {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub mac: String,
    #[serde(default = "default_unknown")]
    pub model: String,
    #[serde(default = "default_gen2", rename = "gen")]
    pub generation: u64,
    #[serde(default = "default_unknown")]
    pub ver: String,
    #[serde(default)]
    pub auth_en: bool,
    pub app: Option<String>,
    pub name: Option<String>,
}

/// Gen2 Switch component status.
#[derive(Debug, Deserialize)]
pub struct Gen2SwitchStatus {
    #[serde(default)]
    pub id: u8,
    #[serde(default)]
    pub output: bool,
    pub source: Option<String>,
    pub apower: Option<f64>,
    pub voltage: Option<f64>,
    pub current: Option<f64>,
    #[serde(rename = "freq")]
    pub frequency: Option<f64>,
    pub temperature: Option<Gen2SwitchTemperature>,
    pub aenergy: Option<Gen2Energy>,
    pub timer_started_at: Option<f64>,
    pub timer_duration: Option<f64>,
}

/// Gen2 switch temperature sub-object.
#[derive(Debug, Deserialize)]
pub struct Gen2SwitchTemperature {
    #[serde(rename = "tC")]
    pub t_c: Option<f64>,
}

/// Gen2 energy sub-object.
#[derive(Debug, Deserialize)]
pub struct Gen2Energy {
    pub total: Option<f64>,
}

/// Gen2 Wifi component status.
#[derive(Debug, Deserialize)]
pub struct Gen2WifiStatus {
    pub sta_ip: Option<String>,
    pub status: Option<String>,
    pub ssid: Option<String>,
    pub rssi: Option<i64>,
}

/// Gen2 Sys component status.
#[derive(Debug, Deserialize)]
pub struct Gen2SysStatus {
    pub uptime: Option<u64>,
    pub time: Option<String>,
    pub ram_free: Option<u64>,
}

/// Gen2 cloud block.
#[derive(Debug, Deserialize)]
pub struct Gen2Cloud {
    pub connected: Option<bool>,
}

/// Gen2 mqtt block.
#[derive(Debug, Deserialize)]
pub struct Gen2Mqtt {
    pub connected: Option<bool>,
}

/// Gen2 input component.
#[derive(Debug, Deserialize)]
pub struct Gen2InputStatus {
    #[serde(default)]
    pub id: u8,
    #[serde(default)]
    pub state: bool,
}

fn default_unknown() -> String {
    "unknown".to_string()
}

fn default_gen2() -> u64 {
    2
}
