use serde::{Deserialize, Serialize};

use super::gen1_responses::{Gen1Meter, Gen1Relay, Gen1StatusResponse};
use super::gen2_responses::{Gen2InputStatus, Gen2SwitchStatus};

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
    pub fn from_gen1_relay(id: u8, relay: &Gen1Relay, meter: Option<&Gen1Meter>) -> Self {
        let timer_remaining = if relay.has_timer {
            relay.timer_remaining
        } else {
            None
        };

        let (power_watts, total_energy_wh) = if let Some(m) = meter {
            (m.power, m.total)
        } else {
            (None, None)
        };

        Self {
            id,
            output: relay.ison,
            source: relay.source.clone(),
            power_watts,
            voltage: None,
            current: None,
            frequency: None,
            temperature_c: None,
            total_energy_wh,
            timer_active: relay.has_timer,
            timer_remaining,
        }
    }

    pub fn from_gen2_switch(sw: &Gen2SwitchStatus) -> Self {
        let temperature_c = sw.temperature.as_ref().and_then(|t| t.t_c);
        let total_energy_wh = sw.aenergy.as_ref().and_then(|e| e.total);
        let timer_active = sw.timer_started_at.is_some() && sw.timer_duration.is_some();

        Self {
            id: sw.id,
            output: sw.output,
            source: sw.source.clone(),
            power_watts: sw.apower,
            voltage: sw.voltage,
            current: sw.current,
            frequency: sw.frequency,
            temperature_c,
            total_energy_wh,
            timer_active,
            timer_remaining: sw.timer_duration,
        }
    }

    /// Parse from raw Gen1 JSON (used by API layer that still works with raw values).
    pub fn from_gen1_relay_json(
        id: u8,
        relay: &serde_json::Value,
        meter: Option<&serde_json::Value>,
    ) -> Self {
        let relay: Gen1Relay =
            serde_json::from_value(relay.clone()).unwrap_or(Gen1Relay {
                ison: false,
                source: None,
                has_timer: false,
                timer_remaining: None,
            });
        let meter: Option<Gen1Meter> =
            meter.and_then(|m| serde_json::from_value(m.clone()).ok());
        Self::from_gen1_relay(id, &relay, meter.as_ref())
    }

    /// Parse from raw Gen2 JSON (used by API layer that still works with raw values).
    pub fn from_gen2_switch_json(sw: &serde_json::Value) -> Self {
        let sw: Gen2SwitchStatus =
            serde_json::from_value(sw.clone()).unwrap_or(Gen2SwitchStatus {
                id: 0,
                output: false,
                source: None,
                apower: None,
                voltage: None,
                current: None,
                frequency: None,
                temperature: None,
                aenergy: None,
                timer_started_at: None,
                timer_duration: None,
            });
        Self::from_gen2_switch(&sw)
    }
}

impl DeviceStatus {
    pub fn from_gen1(status: &serde_json::Value) -> Self {
        let resp: Gen1StatusResponse = serde_json::from_value(status.clone())
            .unwrap_or(Gen1StatusResponse {
                relays: Vec::new(),
                meters: Vec::new(),
                inputs: Vec::new(),
                wifi_sta: None,
                uptime: None,
                time: None,
                cloud: None,
                mqtt: None,
                ram_free: None,
                tmp: None,
                temperature: None,
            });

        let switches: Vec<SwitchStatus> = resp
            .relays
            .iter()
            .enumerate()
            .map(|(i, relay)| {
                let meter = resp.meters.get(i);
                SwitchStatus::from_gen1_relay(i as u8, relay, meter)
            })
            .collect();

        let inputs: Vec<InputStatus> = resp
            .inputs
            .iter()
            .enumerate()
            .map(|(i, input)| InputStatus {
                id: i as u8,
                state: input.input != 0,
            })
            .collect();

        let wifi = resp.wifi_sta.map(|w| WifiStatus {
            connected: w.connected,
            ssid: w.ssid,
            ip: w.ip,
            rssi: w.rssi.map(|v| v as i32),
        });

        let temperature_c = resp
            .tmp
            .and_then(|t| t.t_c)
            .or(resp.temperature);

        Self {
            switches,
            inputs,
            wifi,
            uptime: resp.uptime,
            time: resp.time,
            cloud_connected: resp.cloud.and_then(|c| c.connected),
            mqtt_connected: resp.mqtt.and_then(|m| m.connected),
            ram_free: resp.ram_free,
            temperature_c,
        }
    }

    pub fn from_gen2(status: &serde_json::Value) -> Self {
        let mut switches = Vec::new();
        let mut inputs = Vec::new();

        // Gen2/3 status has dynamic keys like "switch:0", "input:0"
        for (key, value) in status.as_object().into_iter().flatten() {
            if key.starts_with("switch:") {
                if let Ok(sw) = serde_json::from_value::<Gen2SwitchStatus>(value.clone()) {
                    switches.push(SwitchStatus::from_gen2_switch(&sw));
                }
            } else if key.starts_with("input:")
                && let Ok(input) = serde_json::from_value::<Gen2InputStatus>(value.clone())
            {
                inputs.push(InputStatus {
                    id: input.id,
                    state: input.state,
                });
            }
        }

        switches.sort_by_key(|s| s.id);
        inputs.sort_by_key(|i| i.id);

        let wifi = status.get("wifi").and_then(|w| {
            serde_json::from_value::<super::gen2_responses::Gen2WifiStatus>(w.clone()).ok()
        }).map(|w| WifiStatus {
            connected: w.status.as_deref() == Some("got ip"),
            ssid: w.ssid,
            ip: w.sta_ip,
            rssi: w.rssi.map(|v| v as i32),
        });

        let sys = status.get("sys").and_then(|s| {
            serde_json::from_value::<super::gen2_responses::Gen2SysStatus>(s.clone()).ok()
        });

        let cloud_connected = status.get("cloud").and_then(|c| {
            serde_json::from_value::<super::gen2_responses::Gen2Cloud>(c.clone()).ok()
        }).and_then(|c| c.connected);

        let mqtt_connected = status.get("mqtt").and_then(|m| {
            serde_json::from_value::<super::gen2_responses::Gen2Mqtt>(m.clone()).ok()
        }).and_then(|m| m.connected);

        let temperature_c = switches.first().and_then(|s| s.temperature_c);

        Self {
            switches,
            inputs,
            wifi,
            uptime: sys.as_ref().and_then(|s| s.uptime),
            time: sys.as_ref().and_then(|s| s.time.clone()),
            cloud_connected,
            mqtt_connected,
            ram_free: sys.and_then(|s| s.ram_free),
            temperature_c,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn gen1_full_status() {
        let status = json!({
            "relays": [
                {"ison": true, "source": "http", "has_timer": false, "timer_remaining": 0}
            ],
            "meters": [
                {"power": 42.5, "total": 12345.6}
            ],
            "inputs": [
                {"input": 1}
            ],
            "wifi_sta": {
                "connected": true,
                "ssid": "MyNetwork",
                "ip": "10.10.20.5",
                "rssi": -58
            },
            "uptime": 86400,
            "time": "14:30",
            "cloud": {"connected": true},
            "mqtt": {"connected": false},
            "ram_free": 32000,
            "tmp": {"tC": 38.5, "tF": 101.3, "is_valid": true}
        });

        let ds = DeviceStatus::from_gen1(&status);
        assert_eq!(ds.switches.len(), 1);
        assert!(ds.switches[0].output);
        assert_eq!(ds.switches[0].source.as_deref(), Some("http"));
        assert_eq!(ds.switches[0].power_watts, Some(42.5));
        assert_eq!(ds.switches[0].total_energy_wh, Some(12345.6));
        assert!(!ds.switches[0].timer_active);

        assert_eq!(ds.inputs.len(), 1);
        assert!(ds.inputs[0].state);

        let wifi = ds.wifi.unwrap();
        assert!(wifi.connected);
        assert_eq!(wifi.ssid.as_deref(), Some("MyNetwork"));
        assert_eq!(wifi.ip.as_deref(), Some("10.10.20.5"));
        assert_eq!(wifi.rssi, Some(-58));

        assert_eq!(ds.uptime, Some(86400));
        assert_eq!(ds.time.as_deref(), Some("14:30"));
        assert_eq!(ds.cloud_connected, Some(true));
        assert_eq!(ds.mqtt_connected, Some(false));
        assert_eq!(ds.ram_free, Some(32000));
        assert_eq!(ds.temperature_c, Some(38.5));
    }

    #[test]
    fn gen2_full_status() {
        let status = json!({
            "switch:0": {
                "id": 0,
                "source": "WS",
                "output": true,
                "apower": 100.5,
                "voltage": 224.0,
                "current": 0.45,
                "freq": 50.0,
                "temperature": {"tC": 42.0, "tF": 107.6},
                "aenergy": {"total": 5678.9, "by_minute": [0.0], "minute_ts": 0}
            },
            "input:0": {
                "id": 0,
                "state": true
            },
            "wifi": {
                "sta_ip": "10.10.20.10",
                "status": "got ip",
                "ssid": "HomeNet",
                "rssi": -45
            },
            "sys": {
                "uptime": 3600,
                "time": "09:15",
                "ram_free": 64000
            },
            "cloud": {"connected": true},
            "mqtt": {"connected": true}
        });

        let ds = DeviceStatus::from_gen2(&status);
        assert_eq!(ds.switches.len(), 1);
        assert!(ds.switches[0].output);
        assert_eq!(ds.switches[0].source.as_deref(), Some("WS"));
        assert_eq!(ds.switches[0].power_watts, Some(100.5));
        assert_eq!(ds.switches[0].voltage, Some(224.0));
        assert_eq!(ds.switches[0].current, Some(0.45));
        assert_eq!(ds.switches[0].frequency, Some(50.0));
        assert_eq!(ds.switches[0].temperature_c, Some(42.0));
        assert_eq!(ds.switches[0].total_energy_wh, Some(5678.9));

        assert_eq!(ds.inputs.len(), 1);
        assert!(ds.inputs[0].state);

        let wifi = ds.wifi.unwrap();
        assert!(wifi.connected);
        assert_eq!(wifi.ssid.as_deref(), Some("HomeNet"));
        assert_eq!(wifi.ip.as_deref(), Some("10.10.20.10"));
        assert_eq!(wifi.rssi, Some(-45));

        assert_eq!(ds.uptime, Some(3600));
        assert_eq!(ds.time.as_deref(), Some("09:15"));
        assert_eq!(ds.cloud_connected, Some(true));
        assert_eq!(ds.mqtt_connected, Some(true));
        assert_eq!(ds.ram_free, Some(64000));
        assert_eq!(ds.temperature_c, Some(42.0));
    }

    #[test]
    fn gen1_minimal_status() {
        let status = json!({});

        let ds = DeviceStatus::from_gen1(&status);
        assert!(ds.switches.is_empty());
        assert!(ds.inputs.is_empty());
        assert!(ds.wifi.is_none());
        assert!(ds.uptime.is_none());
        assert!(ds.temperature_c.is_none());
    }

    #[test]
    fn gen2_minimal_status() {
        let status = json!({});

        let ds = DeviceStatus::from_gen2(&status);
        assert!(ds.switches.is_empty());
        assert!(ds.inputs.is_empty());
        assert!(ds.wifi.is_none());
        assert!(ds.uptime.is_none());
    }

    #[test]
    fn gen1_multiple_relays() {
        let status = json!({
            "relays": [
                {"ison": true, "source": "http", "has_timer": false},
                {"ison": false, "source": "switch", "has_timer": true, "timer_remaining": 30.0}
            ],
            "meters": [
                {"power": 10.0, "total": 100.0},
                {"power": 20.0, "total": 200.0}
            ]
        });

        let ds = DeviceStatus::from_gen1(&status);
        assert_eq!(ds.switches.len(), 2);

        assert!(ds.switches[0].output);
        assert_eq!(ds.switches[0].id, 0);
        assert_eq!(ds.switches[0].power_watts, Some(10.0));

        assert!(!ds.switches[1].output);
        assert_eq!(ds.switches[1].id, 1);
        assert_eq!(ds.switches[1].power_watts, Some(20.0));
        assert!(ds.switches[1].timer_active);
        assert_eq!(ds.switches[1].timer_remaining, Some(30.0));
    }

    #[test]
    fn gen2_multiple_switches_sorted() {
        let status = json!({
            "switch:1": {"id": 1, "output": false},
            "switch:0": {"id": 0, "output": true}
        });

        let ds = DeviceStatus::from_gen2(&status);
        assert_eq!(ds.switches.len(), 2);
        assert_eq!(ds.switches[0].id, 0);
        assert!(ds.switches[0].output);
        assert_eq!(ds.switches[1].id, 1);
        assert!(!ds.switches[1].output);
    }

    #[test]
    fn gen1_temperature_fallback() {
        // When tmp is missing, falls back to top-level temperature field
        let status = json!({
            "temperature": 35.0
        });
        let ds = DeviceStatus::from_gen1(&status);
        assert_eq!(ds.temperature_c, Some(35.0));
    }

    #[test]
    fn gen2_wifi_not_connected() {
        let status = json!({
            "wifi": {
                "sta_ip": null,
                "status": "disconnected",
                "ssid": null,
                "rssi": -90
            }
        });
        let ds = DeviceStatus::from_gen2(&status);
        let wifi = ds.wifi.unwrap();
        assert!(!wifi.connected);
    }

    #[test]
    fn gen2_timer_active() {
        let status = json!({
            "switch:0": {
                "id": 0,
                "output": true,
                "timer_started_at": 1000.0,
                "timer_duration": 60.0
            }
        });

        let ds = DeviceStatus::from_gen2(&status);
        assert!(ds.switches[0].timer_active);
        assert_eq!(ds.switches[0].timer_remaining, Some(60.0));
    }
}
