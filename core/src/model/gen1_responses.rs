use serde::Deserialize;

/// Gen1 `/shelly` endpoint response.
#[derive(Debug, Deserialize)]
pub struct Gen1ShellyResponse {
    #[serde(rename = "type")]
    pub device_type: String,
    #[serde(default)]
    pub mac: String,
    #[serde(default)]
    pub auth: bool,
    #[serde(default = "default_unknown")]
    pub fw: String,
    #[serde(default = "default_one")]
    pub num_outputs: u8,
    #[serde(default)]
    pub num_meters: u8,
}

/// Gen1 `/status` relay entry.
#[derive(Debug, Deserialize)]
pub struct Gen1Relay {
    #[serde(default)]
    pub ison: bool,
    pub source: Option<String>,
    #[serde(default)]
    pub has_timer: bool,
    pub timer_remaining: Option<f64>,
}

/// Gen1 `/status` meter entry.
#[derive(Debug, Deserialize)]
pub struct Gen1Meter {
    pub power: Option<f64>,
    pub total: Option<f64>,
}

/// Gen1 `/status` wifi_sta.
#[derive(Debug, Deserialize)]
pub struct Gen1WifiSta {
    #[serde(default)]
    pub connected: bool,
    pub ssid: Option<String>,
    pub ip: Option<String>,
    pub rssi: Option<i64>,
}

/// Gen1 `/status` temperature block.
#[derive(Debug, Deserialize)]
pub struct Gen1Temperature {
    #[serde(rename = "tC")]
    pub t_c: Option<f64>,
}

/// Gen1 `/status` cloud block.
#[derive(Debug, Deserialize)]
pub struct Gen1Cloud {
    pub connected: Option<bool>,
}

/// Gen1 `/status` mqtt block.
#[derive(Debug, Deserialize)]
pub struct Gen1Mqtt {
    pub connected: Option<bool>,
}

/// Gen1 `/status` input entry.
#[derive(Debug, Deserialize)]
pub struct Gen1Input {
    #[serde(default)]
    pub input: u64,
}

/// Gen1 `/status` full response.
#[derive(Debug, Deserialize)]
pub struct Gen1StatusResponse {
    #[serde(default)]
    pub relays: Vec<Gen1Relay>,
    #[serde(default)]
    pub meters: Vec<Gen1Meter>,
    #[serde(default)]
    pub inputs: Vec<Gen1Input>,
    pub wifi_sta: Option<Gen1WifiSta>,
    pub uptime: Option<u64>,
    pub time: Option<String>,
    pub cloud: Option<Gen1Cloud>,
    pub mqtt: Option<Gen1Mqtt>,
    pub ram_free: Option<u64>,
    pub tmp: Option<Gen1Temperature>,
    pub temperature: Option<f64>,
}

fn default_unknown() -> String {
    "unknown".to_string()
}

fn default_one() -> u8 {
    1
}
