pub mod discovery;
pub mod gen1;
pub mod gen2;

use std::net::{IpAddr, SocketAddr};

use crate::Result;
use crate::error::Error;
use crate::model::{
    DeviceInfo, DeviceStatus, LightComponent, LightKind, LightParams, LightStatus, PowerReading,
    SwitchStatus,
};

#[derive(Debug, Clone)]
pub struct SwitchResult {
    pub was_on: bool,
}

#[derive(Debug, Clone)]
pub struct FirmwareInfo {
    pub current_version: String,
    pub has_update: bool,
    pub stable_version: Option<String>,
    pub beta_version: Option<String>,
}

pub enum ShellyDevice {
    Gen1(gen1::Gen1Device),
    Gen2(gen2::Gen2Device),
}

/// A Gen1 device does not implement light control at all; every light
/// operation on it is genuinely unsupported.
fn gen1_light_unsupported() -> Error {
    Error::Unsupported {
        message: "light control for Gen1 devices is not yet implemented (planned)".to_string(),
    }
}

impl ShellyDevice {
    pub fn info(&self) -> &DeviceInfo {
        match self {
            Self::Gen1(d) => d.info(),
            Self::Gen2(d) => d.info(),
        }
    }

    pub async fn status(&self) -> Result<DeviceStatus> {
        match self {
            Self::Gen1(d) => d.status().await,
            Self::Gen2(d) => d.status().await,
        }
    }

    pub async fn switch_status(&self, id: u8) -> Result<SwitchStatus> {
        match self {
            Self::Gen1(d) => d.switch_status(id).await,
            Self::Gen2(d) => d.switch_status(id).await,
        }
    }

    pub async fn switch_set(&self, id: u8, on: bool) -> Result<SwitchResult> {
        match self {
            Self::Gen1(d) => d.switch_set(id, on).await,
            Self::Gen2(d) => d.switch_set(id, on).await,
        }
    }

    pub async fn switch_toggle(&self, id: u8) -> Result<SwitchResult> {
        match self {
            Self::Gen1(d) => d.switch_toggle(id).await,
            Self::Gen2(d) => d.switch_toggle(id).await,
        }
    }

    pub async fn light_components(&self) -> Result<Vec<LightComponent>> {
        match self {
            Self::Gen1(_) => Err(gen1_light_unsupported()),
            Self::Gen2(d) => d.light_components().await,
        }
    }

    pub async fn light_set(
        &self,
        kind: LightKind,
        id: u8,
        params: &LightParams,
    ) -> Result<SwitchResult> {
        match self {
            Self::Gen1(_) => Err(gen1_light_unsupported()),
            Self::Gen2(d) => d.light_set(kind, id, params).await,
        }
    }

    pub async fn light_toggle(&self, kind: LightKind, id: u8) -> Result<SwitchResult> {
        match self {
            Self::Gen1(_) => Err(gen1_light_unsupported()),
            Self::Gen2(d) => d.light_toggle(kind, id).await,
        }
    }

    pub async fn light_status(&self, kind: LightKind, id: u8) -> Result<LightStatus> {
        match self {
            Self::Gen1(_) => Err(gen1_light_unsupported()),
            Self::Gen2(d) => d.light_status(kind, id).await,
        }
    }

    pub async fn power(&self, id: u8) -> Result<PowerReading> {
        match self {
            Self::Gen1(d) => d.power(id).await,
            Self::Gen2(d) => d.power(id).await,
        }
    }

    pub async fn firmware_check(&self) -> Result<FirmwareInfo> {
        match self {
            Self::Gen1(d) => d.firmware_check().await,
            Self::Gen2(d) => d.firmware_check().await,
        }
    }

    pub async fn config_get(&self) -> Result<serde_json::Value> {
        match self {
            Self::Gen1(d) => d.config_get().await,
            Self::Gen2(d) => d.config_get().await,
        }
    }

    pub async fn reboot(&self) -> Result<()> {
        match self {
            Self::Gen1(d) => d.reboot().await,
            Self::Gen2(d) => d.reboot().await,
        }
    }

    pub async fn firmware_update(&self) -> Result<()> {
        match self {
            Self::Gen1(d) => d.firmware_update().await,
            Self::Gen2(d) => d.firmware_update().await,
        }
    }

    pub async fn config_set(&self, key: &str, value: &str) -> Result<serde_json::Value> {
        match self {
            Self::Gen1(d) => d.config_set(key, value).await,
            Self::Gen2(d) => d.config_set(key, value).await,
        }
    }

    pub async fn schedule_list(&self) -> Result<serde_json::Value> {
        match self {
            Self::Gen1(d) => d.schedule_list().await,
            Self::Gen2(d) => d.schedule_list().await,
        }
    }

    pub async fn webhook_list(&self) -> Result<serde_json::Value> {
        match self {
            Self::Gen1(d) => d.webhook_list().await,
            Self::Gen2(d) => d.webhook_list().await,
        }
    }

    pub async fn config_restore(&self, config: &serde_json::Value) -> Result<()> {
        match self {
            Self::Gen1(d) => d.config_restore(config).await,
            Self::Gen2(d) => d.config_restore(config).await,
        }
    }

    pub async fn set_name(&self, name: &str) -> Result<()> {
        match self {
            Self::Gen1(d) => d.set_name(name).await,
            Self::Gen2(d) => d.set_name(name).await,
        }
    }
}

pub fn create_device(
    info: DeviceInfo,
    client: reqwest::Client,
    password: Option<String>,
) -> ShellyDevice {
    let base_host = info.ip.to_string();
    create_device_with_host(info, base_host, client, password)
}

/// Build a `ShellyDevice` that addresses the device via an explicit
/// `host[:port]` string rather than `info.ip`. `info` still carries the
/// device identity (model, generation, mac, ...); `base_host` is only used
/// to build the HTTP/RPC URLs, so it can carry a port that `info.ip` cannot.
pub fn create_device_with_host(
    info: DeviceInfo,
    base_host: String,
    client: reqwest::Client,
    password: Option<String>,
) -> ShellyDevice {
    match info.generation {
        crate::model::DeviceGeneration::Gen1 => ShellyDevice::Gen1(
            gen1::Gen1Device::new_with_host(info, base_host, client, password),
        ),
        crate::model::DeviceGeneration::Gen2 | crate::model::DeviceGeneration::Gen3 => {
            ShellyDevice::Gen2(gen2::Gen2Device::new_with_host(
                info, base_host, client, password,
            ))
        }
    }
}

/// Probe a device by IP address. Thin wrapper around `probe_target` that
/// exists so existing callers (CLI subnet discovery) keep working unchanged.
pub async fn probe_device(ip: IpAddr, client: &reqwest::Client) -> Result<DeviceInfo> {
    probe_target(&ip.to_string(), client).await
}

/// Probe a device by `host[:port]` string, fetching `/shelly` and
/// classifying the outcome uniformly with every other transport path:
/// unreachable/non-success -> `Network` (or `Auth` for 401/403), a JSON body
/// that fails to decode -> `Parse`, and a JSON body that decodes but does
/// not describe a Shelly device -> `Parse` as well (reachable, answered,
/// just not a Shelly). A `/shelly` response that DOES describe a Shelly, on
/// a `host` that isn't an IP address (hostname/mDNS) -> `Unsupported`: the
/// device is real, so this must not collapse into the "not a Shelly" case.
/// This is what lets a caller distinguish "nothing there" (`Network`/`Err`)
/// from "something else is there" (`Parse`/`Err` still, but a different
/// variant) from "a Shelly is there but this client can't address it yet"
/// (`Unsupported`/`Err`) from "a Shelly is there" (`Ok`).
pub async fn probe_target(host: &str, client: &reqwest::Client) -> Result<DeviceInfo> {
    let url = format!("http://{host}/shelly");
    let resp = client.get(&url).send().await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::error::status_error(status, &url, &body));
    }

    let shelly: serde_json::Value = resp.json().await?;

    // `/shelly` answered - a real device (Shelly or otherwise) is there.
    // A host we can't turn into an `IpAddr` (a hostname/mDNS name) is NOT
    // "reachable, answered, not a Shelly" (`Error::Parse`); that would
    // silently misclassify a real Shelly as absent. It genuinely can't be
    // represented by `DeviceInfo.ip: IpAddr` yet, so report it honestly as
    // unsupported rather than folding it into the not-a-Shelly case.
    let ip = parse_host_ip(host).ok_or_else(|| Error::Unsupported {
        message: format!(
            "host '{host}' is not an IP address; hostname/mDNS targets are not yet supported (use the device IP)"
        ),
    })?;

    let mut info = DeviceInfo::from_shelly_response(ip, &shelly).ok_or_else(|| Error::Parse {
        message: format!("unrecognized Shelly response from {host}"),
    })?;

    // Gen2/Gen3 devices don't report num_outputs in /shelly, so count switch
    // components from the full status response.
    if matches!(
        info.generation,
        crate::model::DeviceGeneration::Gen2 | crate::model::DeviceGeneration::Gen3
    ) && let Ok((num_outputs, num_meters)) = count_gen2_outputs(host, client).await
    {
        info.num_outputs = num_outputs;
        info.num_meters = num_meters;
    }

    Ok(info)
}

/// Resolve a `host` or `host:port` string down to the bare `IpAddr` used to
/// populate `DeviceInfo.ip`. Handles bracketed IPv6 (`[::1]:8080`), bare
/// IPv6 (`::1`), and IPv4 with or without a port.
fn parse_host_ip(host: &str) -> Option<IpAddr> {
    if let Ok(addr) = host.parse::<SocketAddr>() {
        return Some(addr.ip());
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(ip);
    }
    // IPv4 host:port without brackets (SocketAddr parses this too, but be
    // defensive in case of formats it rejects).
    if let Some((host_part, _port)) = host.rsplit_once(':')
        && let Ok(ip) = host_part.parse::<IpAddr>()
    {
        return Some(ip);
    }
    None
}

/// Count switch components from a Gen2/Gen3 `Shelly.GetStatus` response.
async fn count_gen2_outputs(host: &str, client: &reqwest::Client) -> Result<(u8, u8)> {
    let url = format!("http://{host}/rpc/Shelly.GetStatus");
    let resp = client.get(&url).send().await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::error::status_error(status, &url, &body));
    }

    let status: serde_json::Value = resp.json().await?;

    let obj = status.as_object().ok_or_else(|| Error::Parse {
        message: format!("expected a JSON object from {url}"),
    })?;

    let num_switches = obj.keys().filter(|k| k.starts_with("switch:")).count() as u8;

    // Gen2 power metering is embedded in each switch component
    Ok((num_switches.max(1), num_switches.max(1)))
}
