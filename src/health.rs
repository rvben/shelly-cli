use owo_colors::OwoColorize;
use serde::Serialize;

use crate::api;
use crate::model::DeviceInfo;
use crate::output::{format_duration, use_color};

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
    password: &Option<String>,
) -> HealthReport {
    let device = api::create_device(info.clone(), client.clone(), password.clone());
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
            issues.push(format!(
                "temperature {t:.1}\u{00b0}C exceeds {TEMP_CRIT_C}\u{00b0}C"
            ));
            TempStatus::Hot
        }
        Some(t) if t >= TEMP_WARN_C => {
            issues.push(format!(
                "temperature {t:.1}\u{00b0}C above {TEMP_WARN_C}\u{00b0}C"
            ));
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

fn colored_icon(icon: &str, color: bool) -> String {
    if !color {
        return icon.to_string();
    }
    match icon {
        "OK" => "OK".green().bold().to_string(),
        "!!" => "!!".yellow().bold().to_string(),
        "X" => "X".red().bold().to_string(),
        other => other.to_string(),
    }
}

fn colored_temp_status(status: &TempStatus, temp_str: &str, color: bool) -> String {
    if !color {
        return temp_str.to_string();
    }
    match status {
        TempStatus::Normal => temp_str.green().to_string(),
        TempStatus::Warm => temp_str.yellow().to_string(),
        TempStatus::Hot => temp_str.red().to_string(),
        TempStatus::Unknown => temp_str.dimmed().to_string(),
    }
}

fn colored_wifi_status(status: &WifiSignal, rssi_str: &str, color: bool) -> String {
    if !color {
        return rssi_str.to_string();
    }
    match status {
        WifiSignal::Strong => rssi_str.green().to_string(),
        WifiSignal::Good => rssi_str.to_string(),
        WifiSignal::Weak => rssi_str.yellow().to_string(),
        WifiSignal::VeryWeak => rssi_str.red().to_string(),
        WifiSignal::Unknown => rssi_str.dimmed().to_string(),
    }
}

pub fn print_health_report(reports: &[HealthReport]) {
    let color = use_color();
    let mut offline_count = 0;
    let mut issue_count = 0;
    let mut update_count = 0;
    let mut weakest_rssi: Option<(i32, String)> = None;

    for report in reports {
        let (icon_raw, icon_display) = if !report.online {
            offline_count += 1;
            ("X", colored_icon("X", color))
        } else if report.issues.is_empty() {
            ("OK", colored_icon("OK", color))
        } else {
            issue_count += 1;
            ("!!", colored_icon("!!", color))
        };

        // Pad the icon field manually since ANSI codes break format width
        let icon_pad = match icon_raw {
            "OK" => " ",
            "!!" => " ",
            "X" => "  ",
            _ => " ",
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
            .map(|t| format!("{t:.1}\u{00b0}C ({})", report.temperature_status))
            .unwrap_or_else(|| "-".into());
        let temp_display = colored_temp_status(&report.temperature_status, &temp_str, color);

        let rssi_str = report
            .wifi_rssi
            .map(|r| format!("{r} dBm ({})", report.wifi_status))
            .unwrap_or_else(|| "-".into());
        let rssi_display = colored_wifi_status(&report.wifi_status, &rssi_str, color);

        let uptime_str = report
            .uptime_seconds
            .map(format_duration)
            .unwrap_or_else(|| "-".into());

        let device_display = if !report.online && color {
            report.device.red().to_string()
        } else {
            report.device.clone()
        };

        let online_label = if report.online {
            if color {
                "online".green().to_string()
            } else {
                "online".to_string()
            }
        } else if color {
            "OFFLINE".red().bold().to_string()
        } else {
            "OFFLINE".to_string()
        };

        // Use manual padding for fields that contain ANSI codes
        println!(
            " {icon_display}{icon_pad} {device_display:<30} {:<16} temp: {temp_display:<20} wifi: {rssi_display:<22} up: {uptime_str:<12} fw: {}",
            report.ip, report.firmware,
        );

        if !report.online {
            // Print the OFFLINE marker as an issue line
            println!("        {online_label}");
        }

        for issue in &report.issues {
            if color {
                println!("        {}", issue.yellow());
            } else {
                println!("        {issue}");
            }
        }
    }

    println!();

    let total = reports.len();
    let online = total - offline_count;

    if color {
        let online_str = format!("{online}/{total}").green().to_string();
        let issue_str = if issue_count > 0 {
            format!("{issue_count}").yellow().to_string()
        } else {
            format!("{issue_count}")
        };
        let update_str = if update_count > 0 {
            format!("{update_count}").yellow().to_string()
        } else {
            format!("{update_count}")
        };
        println!(
            "Summary: {online_str} online, {issue_str} with issues, {update_str} firmware updates available"
        );
    } else {
        println!(
            "Summary: {online}/{total} online, {issue_count} with issues, {update_count} firmware updates available"
        );
    }

    if let Some((rssi, name)) = weakest_rssi {
        println!("Weakest WiFi: {rssi} dBm ({name})");
    }
}
