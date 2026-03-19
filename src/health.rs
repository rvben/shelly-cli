use serde::Serialize;

use crate::api;
use crate::model::DeviceInfo;

const TEMP_WARN_C: f64 = 60.0;
const TEMP_CRIT_C: f64 = 75.0;
const RSSI_WEAK: i32 = -75;
const RSSI_VERY_WEAK: i32 = -85;

#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    pub device: String,
    pub ip: String,
    pub online: bool,
    pub temperature_c: Option<f64>,
    pub temperature_status: TempStatus,
    pub wifi_rssi: Option<i32>,
    pub wifi_status: WifiSignal,
    pub firmware: String,
    pub has_update: bool,
    pub uptime_seconds: Option<u64>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub enum TempStatus {
    Normal,
    Warm,
    Hot,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub enum WifiSignal {
    Strong,
    Good,
    Weak,
    VeryWeak,
    Unknown,
}

impl std::fmt::Display for TempStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::Warm => write!(f, "WARM"),
            Self::Hot => write!(f, "HOT"),
            Self::Unknown => write!(f, "?"),
        }
    }
}

impl std::fmt::Display for WifiSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Strong => write!(f, "strong"),
            Self::Good => write!(f, "good"),
            Self::Weak => write!(f, "weak"),
            Self::VeryWeak => write!(f, "VERY WEAK"),
            Self::Unknown => write!(f, "?"),
        }
    }
}

pub async fn check_device(
    info: &DeviceInfo,
    client: &reqwest::Client,
) -> HealthReport {
    let device = api::create_device(info.clone(), client.clone());
    let mut issues = Vec::new();

    let (online, temperature_c, wifi_rssi, uptime) = match device.status().await {
        Ok(status) => {
            let temp = status.temperature_c;
            let rssi = status.wifi.as_ref().and_then(|w| w.rssi);
            (true, temp, rssi, status.uptime)
        }
        Err(_) => {
            issues.push("device is offline or unreachable".into());
            (false, None, None, None)
        }
    };

    let temp_status = match temperature_c {
        Some(t) if t >= TEMP_CRIT_C => {
            issues.push(format!("temperature {t:.1}°C exceeds {TEMP_CRIT_C}°C"));
            TempStatus::Hot
        }
        Some(t) if t >= TEMP_WARN_C => {
            issues.push(format!("temperature {t:.1}°C above {TEMP_WARN_C}°C"));
            TempStatus::Warm
        }
        Some(_) => TempStatus::Normal,
        None => TempStatus::Unknown,
    };

    let wifi_status = match wifi_rssi {
        Some(r) if r <= RSSI_VERY_WEAK => {
            issues.push(format!("very weak WiFi signal ({r} dBm)"));
            WifiSignal::VeryWeak
        }
        Some(r) if r <= RSSI_WEAK => {
            issues.push(format!("weak WiFi signal ({r} dBm)"));
            WifiSignal::Weak
        }
        Some(r) if r <= -60 => WifiSignal::Good,
        Some(_) => WifiSignal::Strong,
        None => WifiSignal::Unknown,
    };

    let (firmware, has_update) = if online {
        match device.firmware_check().await {
            Ok(fw) => {
                if fw.has_update
                    && let Some(ref stable) = fw.stable_version
                {
                    issues.push(format!("firmware update available: {stable}"));
                }
                (fw.current_version, fw.has_update)
            }
            Err(_) => (info.firmware_version.clone(), false),
        }
    } else {
        (info.firmware_version.clone(), false)
    };

    HealthReport {
        device: info.display_name().to_string(),
        ip: info.ip.to_string(),
        online,
        temperature_c,
        temperature_status: temp_status,
        wifi_rssi,
        wifi_status,
        firmware,
        has_update,
        uptime_seconds: uptime,
        issues,
    }
}

pub fn print_health_report(reports: &[HealthReport]) {
    let mut offline_count = 0;
    let mut issue_count = 0;
    let mut update_count = 0;
    let mut weakest_rssi: Option<(i32, String)> = None;

    for report in reports {
        let icon = if !report.online {
            offline_count += 1;
            "x"
        } else if report.issues.is_empty() {
            "ok"
        } else {
            issue_count += 1;
            "!!"
        };

        if report.has_update {
            update_count += 1;
        }

        if let Some(rssi) = report.wifi_rssi
            && weakest_rssi.as_ref().is_none_or(|(r, _)| rssi < *r)
        {
            weakest_rssi = Some((rssi, report.device.clone()));
        }

        let temp_str = report
            .temperature_c
            .map(|t| format!("{t:.1}°C ({})", report.temperature_status))
            .unwrap_or_else(|| "-".into());

        let rssi_str = report
            .wifi_rssi
            .map(|r| format!("{r} dBm ({})", report.wifi_status))
            .unwrap_or_else(|| "-".into());

        let uptime_str = report
            .uptime_seconds
            .map(format_duration)
            .unwrap_or_else(|| "-".into());

        println!(
            " {icon:<2}  {:<30} {:<16} temp: {:<20} wifi: {:<22} up: {:<12} fw: {}",
            report.device,
            report.ip,
            temp_str,
            rssi_str,
            uptime_str,
            report.firmware,
        );

        for issue in &report.issues {
            println!("        {issue}");
        }
    }

    println!();

    let total = reports.len();
    let online = total - offline_count;
    println!("Summary: {online}/{total} online, {issue_count} with issues, {update_count} firmware updates available");

    if let Some((rssi, name)) = weakest_rssi {
        println!("Weakest WiFi: {rssi} dBm ({name})");
    }
}

fn format_duration(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;

    if days > 0 {
        format!("{days}d {hours}h")
    } else {
        format!("{hours}h")
    }
}
