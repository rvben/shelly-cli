use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchStatus {
    pub id: u8,
    pub output: bool,
    pub source: Option<String>,
    pub power_watts: Option<f64>,
    pub voltage: Option<f64>,
    pub current: Option<f64>,
    pub frequency: Option<f64>,
    pub temperature_c: Option<f64>,
    pub total_energy_wh: Option<f64>,
    pub timer_active: bool,
    pub timer_remaining: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputStatus {
    pub id: u8,
    pub state: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiStatus {
    pub connected: bool,
    pub ssid: Option<String>,
    pub ip: Option<String>,
    pub rssi: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub switches: Vec<SwitchStatus>,
    pub inputs: Vec<InputStatus>,
    pub wifi: Option<WifiStatus>,
    pub uptime: Option<u64>,
    pub time: Option<String>,
    pub cloud_connected: Option<bool>,
    pub mqtt_connected: Option<bool>,
    pub ram_free: Option<u64>,
    pub temperature_c: Option<f64>,
}

impl SwitchStatus {
    pub fn from_gen1_relay(id: u8, relay: &serde_json::Value, meter: Option<&serde_json::Value>) -> Self {
        let output = relay.get("ison").and_then(|v| v.as_bool()).unwrap_or(false);
        let source = relay.get("source").and_then(|v| v.as_str()).map(String::from);
        let has_timer = relay.get("has_timer").and_then(|v| v.as_bool()).unwrap_or(false);
        let timer_remaining = if has_timer {
            relay.get("timer_remaining").and_then(|v| v.as_f64())
        } else {
            None
        };

        let (power_watts, total_energy_wh) = if let Some(m) = meter {
            (
                m.get("power").and_then(|v| v.as_f64()),
                m.get("total").and_then(|v| v.as_f64()),
            )
        } else {
            (None, None)
        };

        Self {
            id,
            output,
            source,
            power_watts,
            voltage: None,
            current: None,
            frequency: None,
            temperature_c: None,
            total_energy_wh,
            timer_active: has_timer,
            timer_remaining,
        }
    }

    pub fn from_gen2_switch(sw: &serde_json::Value) -> Self {
        let id = sw.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
        let output = sw.get("output").and_then(|v| v.as_bool()).unwrap_or(false);
        let source = sw.get("source").and_then(|v| v.as_str()).map(String::from);
        let power_watts = sw.get("apower").and_then(|v| v.as_f64());
        let voltage = sw.get("voltage").and_then(|v| v.as_f64());
        let current = sw.get("current").and_then(|v| v.as_f64());
        let frequency = sw.get("freq").and_then(|v| v.as_f64());

        let temperature_c = sw
            .get("temperature")
            .and_then(|v| v.get("tC"))
            .and_then(|v| v.as_f64());

        let total_energy_wh = sw
            .get("aenergy")
            .and_then(|v| v.get("total"))
            .and_then(|v| v.as_f64());

        let timer_active = sw.get("timer_started_at").is_some()
            && sw.get("timer_duration").is_some();
        let timer_remaining = sw.get("timer_duration").and_then(|v| v.as_f64());

        Self {
            id,
            output,
            source,
            power_watts,
            voltage,
            current,
            frequency,
            temperature_c,
            total_energy_wh,
            timer_active,
            timer_remaining,
        }
    }
}

impl DeviceStatus {
    pub fn from_gen1(status: &serde_json::Value) -> Self {
        let mut switches = Vec::new();
        if let Some(relays) = status.get("relays").and_then(|v| v.as_array()) {
            let meters = status.get("meters").and_then(|v| v.as_array());
            for (i, relay) in relays.iter().enumerate() {
                let meter = meters.and_then(|m| m.get(i));
                switches.push(SwitchStatus::from_gen1_relay(i as u8, relay, meter));
            }
        }

        let mut inputs = Vec::new();
        if let Some(input_arr) = status.get("inputs").and_then(|v| v.as_array()) {
            for (i, input) in input_arr.iter().enumerate() {
                let state = input.get("input").and_then(|v| v.as_u64()).unwrap_or(0) != 0;
                inputs.push(InputStatus {
                    id: i as u8,
                    state,
                });
            }
        }

        let wifi = status.get("wifi_sta").map(|w| WifiStatus {
            connected: w.get("connected").and_then(|v| v.as_bool()).unwrap_or(false),
            ssid: w.get("ssid").and_then(|v| v.as_str()).map(String::from),
            ip: w.get("ip").and_then(|v| v.as_str()).map(String::from),
            rssi: w.get("rssi").and_then(|v| v.as_i64()).map(|v| v as i32),
        });

        let uptime = status.get("uptime").and_then(|v| v.as_u64());
        let time = status.get("time").and_then(|v| v.as_str()).map(String::from);

        let cloud_connected = status
            .get("cloud")
            .and_then(|v| v.get("connected"))
            .and_then(|v| v.as_bool());
        let mqtt_connected = status
            .get("mqtt")
            .and_then(|v| v.get("connected"))
            .and_then(|v| v.as_bool());
        let ram_free = status.get("ram_free").and_then(|v| v.as_u64());

        let temperature_c = status
            .get("tmp")
            .and_then(|v| v.get("tC"))
            .and_then(|v| v.as_f64())
            .or_else(|| status.get("temperature").and_then(|v| v.as_f64()));

        Self {
            switches,
            inputs,
            wifi,
            uptime,
            time,
            cloud_connected,
            mqtt_connected,
            ram_free,
            temperature_c,
        }
    }

    pub fn from_gen2(status: &serde_json::Value) -> Self {
        let mut switches = Vec::new();
        let mut inputs = Vec::new();

        // Gen2/3 status has keys like "switch:0", "input:0"
        for (key, value) in status.as_object().into_iter().flatten() {
            if key.starts_with("switch:") {
                switches.push(SwitchStatus::from_gen2_switch(value));
            } else if key.starts_with("input:") {
                let id = value.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
                let state = value.get("state").and_then(|v| v.as_bool()).unwrap_or(false);
                inputs.push(InputStatus { id, state });
            }
        }

        switches.sort_by_key(|s| s.id);
        inputs.sort_by_key(|i| i.id);

        let wifi = status.get("wifi").map(|w| WifiStatus {
            connected: w.get("status").and_then(|v| v.as_str()) == Some("got ip"),
            ssid: w.get("ssid").and_then(|v| v.as_str()).map(String::from),
            ip: w.get("sta_ip").and_then(|v| v.as_str()).map(String::from),
            rssi: w.get("rssi").and_then(|v| v.as_i64()).map(|v| v as i32),
        });

        let sys = status.get("sys");
        let uptime = sys.and_then(|s| s.get("uptime")).and_then(|v| v.as_u64());
        let time = sys
            .and_then(|s| s.get("time"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let ram_free = sys
            .and_then(|s| s.get("ram_free"))
            .and_then(|v| v.as_u64());

        let cloud_connected = status
            .get("cloud")
            .and_then(|v| v.get("connected"))
            .and_then(|v| v.as_bool());
        let mqtt_connected = status
            .get("mqtt")
            .and_then(|v| v.get("connected"))
            .and_then(|v| v.as_bool());

        let temperature_c = switches.first().and_then(|s| s.temperature_c);

        Self {
            switches,
            inputs,
            wifi,
            uptime,
            time,
            cloud_connected,
            mqtt_connected,
            ram_free,
            temperature_c,
        }
    }
}
