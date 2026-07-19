//! `switchkit::SmartDevice` adapter for Shelly Gen1 (REST) and Gen2/3
//! (JSON-RPC) devices.
//!
//! [`ShellyClient`] is stateless: one instance serves every
//! `switchkit::DeviceTarget`. Every trait method opens a fresh
//! [`ShellyDevice`] addressed by `DeviceTarget.host` (never `DeviceInfo.ip`,
//! which drops any port a target carries), then maps the shelly-core
//! response onto switchkit's vendor-neutral types.
//!
//! # Honesty
//! [`snapshot_from`] only ever converts a value the device actually
//! reported. A switch absent from the device's status response is simply
//! absent from `relays` (never a fabricated `Off`); metering fields
//! (`energy`) are `None` unless at least one switch actually reports a
//! metering value; `signal` is only ever built from a real dBm reading via
//! `Signal::from_dbm`, never a guessed percentage. See the field-level
//! comments in `snapshot_from` for the full mapping.

use std::time::Duration;

use serde_json::Value;
use switchkit::{
    Capabilities, DeviceSnapshot, DeviceTarget, Energy, Firmware, NetInfo, PowerAction, Relay,
    RelayState, Signal, SmartDevice, Vendor,
};

use crate::api::{ShellyDevice, create_device_with_host, probe_target};
use crate::error::Error as CoreError;
use crate::model::{DeviceGeneration, DeviceInfo, DeviceStatus};

/// Stateless `switchkit::SmartDevice` adapter for Shelly devices. No
/// per-device state is kept here; every call re-opens the device (a cheap
/// HTTP probe) so the adapter always reflects the device's current
/// generation and identity.
pub struct ShellyClient {
    http: reqwest::Client,
}

impl ShellyClient {
    /// Build a client with a bounded request timeout and connect timeout, so
    /// a dead or unreachable device fails fast instead of hanging a caller
    /// indefinitely.
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_default();
        Self { http }
    }
}

impl Default for ShellyClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Shelly authentication is a single password (HTTP Basic for both
/// generations); `DeviceCredentials.user` is not used by any Shelly
/// transport path.
fn to_password(target: &DeviceTarget) -> Option<String> {
    target.credentials.as_ref().map(|c| c.password.clone())
}

/// Map a `shelly-core` transport error onto its `switchkit` equivalent. The
/// two enums are 1:1 by design (see `shelly_core::error::Error`), so this is
/// a plain re-tag, carrying `host` along for the vendor-neutral error.
fn map_err(err: CoreError, host: &str) -> switchkit::Error {
    let host = host.to_string();
    match err {
        CoreError::Network { message } => switchkit::Error::Network { host, message },
        CoreError::Auth { message } => switchkit::Error::Auth { host, message },
        CoreError::Rejected { message } => switchkit::Error::Rejected { host, message },
        CoreError::Parse { message } => switchkit::Error::Parse { host, message },
        CoreError::Unsupported { message } => switchkit::Error::Unsupported { host, message },
    }
}

/// `DeviceInfo.model`/`firmware_version` default to the sentinel `"unknown"`
/// when the device's `/shelly` response omits them (see
/// `Gen2ShellyResponse::default_unknown`). Reporting that sentinel as a real
/// value would fabricate absent-as-a-plausible-value, so this maps it (and
/// an empty/whitespace-only string) back to `None`.
fn non_sentinel(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unknown") {
        None
    } else {
        Some(s.to_string())
    }
}

/// Map a Shelly `DeviceStatus` (a snapshot from the device, already parsed)
/// onto a vendor-neutral `DeviceSnapshot`. `host` is `DeviceTarget.host`,
/// passed through explicitly rather than derived from `info.ip`, so any port
/// the caller addressed the device by is preserved in the snapshot identity.
fn snapshot_from(host: &str, info: &DeviceInfo, status: DeviceStatus) -> DeviceSnapshot {
    // One `Relay` per switch the device actually reported. A switch id
    // missing from `status.switches` (device offline mid-refresh, a channel
    // that doesn't exist on this model) is simply not in `relays` - never a
    // fabricated `Off`.
    let relays: Vec<Relay> = status
        .switches
        .iter()
        .map(|sw| Relay {
            index: sw.id,
            state: if sw.output {
                RelayState::On
            } else {
                RelayState::Off
            },
            raw: sw.output.to_string(),
        })
        .collect();

    // Metering is `Some` only when the device actually meters: at least one
    // switch reports any of power/voltage/current/total-energy. Values come
    // from `SwitchStatus` (already `Option`, never defaulted), not from
    // `PowerReading`, whose `power_watts`/`total_energy_wh` are `0.0`
    // defaulted and would fabricate a reading on a non-metering device.
    let energy = status
        .switches
        .iter()
        .find(|sw| {
            sw.power_watts.is_some()
                || sw.voltage.is_some()
                || sw.current.is_some()
                || sw.total_energy_wh.is_some()
        })
        .map(|sw| Energy {
            power_w: sw.power_watts,
            voltage_v: sw.voltage,
            current_a: sw.current,
            total_kwh: sw.total_energy_wh.map(|wh| wh / 1000.0),
            // Shelly's status response has no daily energy counter; leaving
            // this `None` is honest, not an oversight.
            today_kwh: None,
        });

    // A real dBm reading via `Signal::from_dbm`, never a fabricated
    // percentage. `WifiStatus.rssi` is `Option<i32>`; `Signal::from_dbm`
    // takes `i64`.
    let signal = status
        .wifi
        .as_ref()
        .and_then(|w| w.rssi)
        .map(|dbm| Signal::from_dbm(i64::from(dbm)));

    let capabilities = Capabilities {
        metering: energy.is_some(),
        multi_channel: status.switches.len() > 1,
        firmware_ota: true,
        config_backup: true,
        // Gen1 has no RPC console at all; report `false` so the app hides
        // the control rather than offering one that always fails.
        console: matches!(
            info.generation,
            DeviceGeneration::Gen2 | DeviceGeneration::Gen3
        ),
    };

    DeviceSnapshot {
        host: host.to_string(),
        name: info.name.clone(),
        // The sentinel `"unknown"` `DeviceInfo` falls back to when the
        // device's `/shelly` response omits `model` is never surfaced as a
        // real value here - only a model the device actually reported.
        model: non_sentinel(&info.model),
        generation: Some(info.generation.to_string()),
        capabilities,
        relays,
        energy,
        signal,
        temperature_c: status.temperature_c,
        // Same sentinel handling for firmware. If there is no real version
        // to report, the whole `Firmware` block is omitted rather than
        // emitting a `Firmware { version: None, .. }` shell that would
        // falsely claim a firmware block exists with an unknown version.
        firmware: non_sentinel(&info.firmware_version).map(|version| Firmware {
            version: Some(version),
            // Shelly's status/info responses don't carry an "update
            // available" flag cheaply alongside them (that's a separate
            // `Shelly.CheckForUpdate` round trip); leave it unknown rather
            // than guessing.
            update_available: None,
        }),
        net: NetInfo {
            ip: Some(info.ip.to_string()),
            mac: Some(info.mac.clone()),
            hostname: None,
        },
        uptime: status.uptime.map(|s| s.to_string()),
    }
}

/// Parse a console command of the form `"Method [json-params]"` into an RPC
/// method name and optional JSON params. Malformed JSON params are reported
/// as `Error::Parse` rather than silently dropped, so a typo in the console
/// input never turns into an unintended parameterless call.
fn parse_console_command(command: &str, host: &str) -> switchkit::Result<(String, Option<Value>)> {
    let trimmed = command.trim();
    let (method, rest) = match trimmed.split_once(char::is_whitespace) {
        Some((method, rest)) => (method, rest.trim()),
        None => (trimmed, ""),
    };

    if rest.is_empty() {
        return Ok((method.to_string(), None));
    }

    let params = serde_json::from_str(rest).map_err(|e| switchkit::Error::Parse {
        host: host.to_string(),
        message: format!("invalid JSON params in console command: {e}"),
    })?;

    Ok((method.to_string(), Some(params)))
}

impl ShellyClient {
    /// Probe `target` for its generation and open a `ShellyDevice` addressed
    /// by `target.host` (with any port), not `info.ip`.
    async fn open(&self, target: &DeviceTarget) -> switchkit::Result<ShellyDevice> {
        let info = probe_target(&target.host, &self.http)
            .await
            .map_err(|e| map_err(e, &target.host))?;

        Ok(create_device_with_host(
            info,
            target.host.clone(),
            self.http.clone(),
            to_password(target),
        ))
    }
}

#[async_trait::async_trait]
impl SmartDevice for ShellyClient {
    fn vendor(&self) -> Vendor {
        Vendor::Shelly
    }

    /// Reachable-but-not-Shelly (`probe_target` returning `Error::Parse`)
    /// maps to `Ok(None)`, never a guessed vendor. Any other error (offline,
    /// auth, ...) propagates as `Err`.
    async fn probe(&self, target: &DeviceTarget) -> switchkit::Result<Option<DeviceSnapshot>> {
        match probe_target(&target.host, &self.http).await {
            Ok(_) => {
                let dev = self.open(target).await?;
                let status = dev.status().await.map_err(|e| map_err(e, &target.host))?;
                Ok(Some(snapshot_from(&target.host, dev.info(), status)))
            }
            Err(CoreError::Parse { .. }) => Ok(None),
            Err(e) => Err(map_err(e, &target.host)),
        }
    }

    async fn status(&self, target: &DeviceTarget) -> switchkit::Result<DeviceSnapshot> {
        let dev = self.open(target).await?;
        let status = dev.status().await.map_err(|e| map_err(e, &target.host))?;
        Ok(snapshot_from(&target.host, dev.info(), status))
    }

    /// Issues the power action, then reads back the CONFIRMED post-change
    /// state via `switch_status` rather than trusting `SwitchResult.was_on`
    /// (the previous state for Gen2's `Switch.Set`/`Switch.Toggle`, with
    /// inconsistent semantics across generations).
    async fn set_power(
        &self,
        target: &DeviceTarget,
        channel: Option<u8>,
        action: PowerAction,
    ) -> switchkit::Result<Relay> {
        let dev = self.open(target).await?;
        let id = channel.unwrap_or(0);

        match action {
            PowerAction::On => dev.switch_set(id, true).await,
            PowerAction::Off => dev.switch_set(id, false).await,
            PowerAction::Toggle => dev.switch_toggle(id).await,
        }
        .map_err(|e| map_err(e, &target.host))?;

        let status = dev
            .switch_status(id)
            .await
            .map_err(|e| map_err(e, &target.host))?;

        let state = if status.output {
            RelayState::On
        } else {
            RelayState::Off
        };

        Ok(Relay {
            index: id,
            state,
            raw: status.output.to_string(),
        })
    }

    async fn firmware_version(&self, target: &DeviceTarget) -> switchkit::Result<Option<String>> {
        let dev = self.open(target).await?;
        Ok(Some(dev.info().firmware_version.clone()))
    }

    /// Shelly's stable-channel OTA update takes no URL parameter; `ota_url`
    /// is accepted for trait compatibility but unused in v1.
    async fn firmware_update(
        &self,
        target: &DeviceTarget,
        _ota_url: Option<&str>,
    ) -> switchkit::Result<()> {
        let dev = self.open(target).await?;
        dev.firmware_update()
            .await
            .map_err(|e| map_err(e, &target.host))?;
        Ok(())
    }

    /// Shelly has no single-setting GET; config is one blob. `setting == ""`
    /// returns the whole config. A non-empty `setting` returns that
    /// top-level key's value when present; an absent key is `Err(Rejected)`,
    /// never a fabricated `null`/`{}` (absent is not a value).
    async fn config_get(&self, target: &DeviceTarget, setting: &str) -> switchkit::Result<Value> {
        let dev = self.open(target).await?;
        let config = dev
            .config_get()
            .await
            .map_err(|e| map_err(e, &target.host))?;

        if setting.is_empty() {
            return Ok(config);
        }

        config
            .get(setting)
            .cloned()
            .ok_or_else(|| switchkit::Error::Rejected {
                host: target.host.clone(),
                message: format!("no such setting `{setting}`"),
            })
    }

    /// Returns the device's actual response to the settings write, never a
    /// fabricated `{"ok":true}`.
    async fn config_set(
        &self,
        target: &DeviceTarget,
        setting: &str,
        value: &str,
    ) -> switchkit::Result<Value> {
        let dev = self.open(target).await?;
        dev.config_set(setting, value)
            .await
            .map_err(|e| map_err(e, &target.host))
    }

    /// A Shelly config backup is its settings JSON, pretty-printed.
    async fn backup(&self, target: &DeviceTarget) -> switchkit::Result<Vec<u8>> {
        let dev = self.open(target).await?;
        let config = dev
            .config_get()
            .await
            .map_err(|e| map_err(e, &target.host))?;
        serde_json::to_vec_pretty(&config).map_err(|e| switchkit::Error::Parse {
            host: target.host.clone(),
            message: format!("failed to serialize config for backup: {e}"),
        })
    }

    /// Gen2/3 only: `command` is `"Method [json-params]"`, passed through
    /// verbatim to the device's JSON-RPC endpoint. Gen1 has no RPC console,
    /// so it is genuinely `Unsupported`, not an empty/guessed response.
    async fn console(&self, target: &DeviceTarget, command: &str) -> switchkit::Result<Value> {
        let dev = self.open(target).await?;
        match dev {
            ShellyDevice::Gen2(ref device) => {
                let (method, params) = parse_console_command(command, &target.host)?;
                device
                    .rpc_raw(&method, params)
                    .await
                    .map_err(|e| map_err(e, &target.host))
            }
            ShellyDevice::Gen1(_) => Err(switchkit::Error::Unsupported {
                host: target.host.clone(),
                message: "Gen1 devices have no RPC console".to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use switchkit::DeviceTarget;

    /// Minimal Gen2 `Shelly.GetStatus` body: no switch components, so
    /// `snapshot_from` builds an empty `relays`/`energy`. Only
    /// model/firmware are under test here.
    fn empty_gen2_status() -> serde_json::Value {
        serde_json::json!({})
    }

    #[tokio::test]
    async fn snapshot_omits_sentinel_model_and_firmware() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/shelly");
                then.status(200).json_body(serde_json::json!({
                    "id": "shellyplus1-abc",
                    "mac": "AABBCCDDEEFF",
                    "gen": 2
                }));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/rpc/Shelly.GetStatus");
                then.status(200).json_body(empty_gen2_status());
            })
            .await;

        let client = ShellyClient::default();
        let target = DeviceTarget::new(server.address().to_string());
        let snapshot = client
            .status(&target)
            .await
            .expect("status should succeed against the mock");

        assert_eq!(
            snapshot.model, None,
            "the 'unknown' sentinel must not be exposed as a real model"
        );
        assert_eq!(
            snapshot.firmware, None,
            "the 'unknown' sentinel must not be exposed as a real firmware version"
        );
    }

    #[tokio::test]
    async fn snapshot_reports_real_model_and_firmware() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/shelly");
                then.status(200).json_body(serde_json::json!({
                    "id": "shellyplus1pm-aabbccddeeff",
                    "mac": "AABBCCDDEEFF",
                    "model": "SNSW-001P16EU",
                    "gen": 2,
                    "ver": "1.2.3",
                    "app": "Plus1PM"
                }));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/rpc/Shelly.GetStatus");
                then.status(200).json_body(empty_gen2_status());
            })
            .await;

        let client = ShellyClient::default();
        let target = DeviceTarget::new(server.address().to_string());
        let snapshot = client
            .status(&target)
            .await
            .expect("status should succeed against the mock");

        assert_eq!(snapshot.model.as_deref(), Some("SNSW-001P16EU"));
        assert_eq!(
            snapshot.firmware.and_then(|f| f.version).as_deref(),
            Some("1.2.3")
        );
    }
}
