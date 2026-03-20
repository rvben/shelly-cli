use std::io::IsTerminal;

use owo_colors::OwoColorize;

use crate::errors::CliError;
use crate::model::{DeviceInfo, DeviceStatus, PowerReading, SwitchStatus};

/// Wrap successful data in a `{"ok": true, "data": ...}` envelope and print to stdout.
pub fn print_json_success<T: serde::Serialize>(data: &T) {
    let envelope = serde_json::json!({
        "ok": true,
        "data": data,
    });
    println!("{}", serde_json::to_string_pretty(&envelope).unwrap());
}

/// Print a structured JSON error envelope to stdout.
pub fn print_json_error(err: &CliError) {
    let envelope = serde_json::json!({
        "ok": false,
        "error": err,
    });
    println!("{}", serde_json::to_string_pretty(&envelope).unwrap());
}

pub fn use_color() -> bool {
    std::io::stdout().is_terminal()
}

/// Shorten firmware version for display: extract version from Gen1's long format.
pub fn short_fw(fw: &str) -> &str {
    // Gen1: "20230913-113709/v1.14.0-gcb84623" → "v1.14.0"
    if let Some(rest) = fw.strip_prefix("20")
        && let Some(slash_pos) = rest.find('/')
    {
        let version_part = &fw[slash_pos + 4..]; // skip "202.../v"
        if let Some(dash) = version_part.find('-') {
            return &version_part[..dash];
        }
        return version_part;
    }
    fw
}

pub fn print_device_table(devices: &[DeviceInfo]) {
    if devices.is_empty() {
        eprintln!("No devices found.");
        return;
    }

    let color = use_color();
    let header = format!(
        "{:<34} {:<16} {:<5} {:<14} {:<12} {}",
        "Name", "IP", "Gen", "Model", "FW", "MAC"
    );
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(97).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(97));
    }

    for d in devices {
        let name = d.display_name();
        let fw = short_fw(&d.firmware_version);
        let gen_str = format!("{}", d.generation);

        if color {
            let gen_colored = match d.generation {
                crate::model::DeviceGeneration::Gen1 => gen_str.dimmed().to_string(),
                crate::model::DeviceGeneration::Gen2 => gen_str.to_string(),
                crate::model::DeviceGeneration::Gen3 => gen_str.green().to_string(),
            };
            println!(
                " {:<33} {:<16} {:<5} {:<14} {:<12} {}",
                name.bold(),
                d.ip,
                gen_colored,
                d.model,
                fw,
                d.mac.dimmed(),
            );
        } else {
            println!(
                " {:<33} {:<16} {:<5} {:<14} {:<12} {}",
                name, d.ip, gen_str, d.model, fw, d.mac,
            );
        }
    }
}

pub fn print_status(name: &str, status: &DeviceStatus) {
    println!("Device: {name}");
    if let Some(time) = &status.time {
        println!("  Time: {time}");
    }
    if let Some(uptime) = status.uptime {
        println!("  Uptime: {}", format_duration(uptime));
    }
    if let Some(wifi) = &status.wifi {
        let rssi_str = wifi.rssi.map_or(String::new(), |r| format!(" (RSSI: {r})"));
        println!(
            "  WiFi: {}{}",
            wifi.ssid.as_deref().unwrap_or("?"),
            rssi_str
        );
    }
    if let Some(cloud) = status.cloud_connected {
        println!(
            "  Cloud: {}",
            if cloud { "connected" } else { "disconnected" }
        );
    }
    if let Some(mqtt) = status.mqtt_connected {
        println!(
            "  MQTT: {}",
            if mqtt { "connected" } else { "disconnected" }
        );
    }
    if let Some(temp) = status.temperature_c {
        println!("  Temperature: {temp:.1}\u{00b0}C");
    }

    for sw in &status.switches {
        print_switch_status(sw);
    }
}

pub fn print_switch_status(sw: &SwitchStatus) {
    let color = use_color();
    let state = if sw.output {
        if color {
            "ON".green().to_string()
        } else {
            "ON".to_string()
        }
    } else if color {
        "OFF".dimmed().to_string()
    } else {
        "OFF".to_string()
    };
    println!("  Switch {}: {state}", sw.id);
    if let Some(power) = sw.power_watts {
        println!("    Power: {power:.1}W");
    }
    if let Some(voltage) = sw.voltage {
        println!("    Voltage: {voltage:.1}V");
    }
    if let Some(current) = sw.current {
        println!("    Current: {current:.3}A");
    }
    if let Some(total) = sw.total_energy_wh {
        println!("    Total energy: {:.2} kWh", total / 1000.0);
    }
    if let Some(temp) = sw.temperature_c {
        println!("    Temperature: {temp:.1}\u{00b0}C");
    }
    if let Some(source) = &sw.source {
        println!("    Last source: {source}");
    }
}

pub fn print_power_reading(name: &str, reading: &PowerReading) {
    println!(
        "{:<30} {:>8.1}W {:>7.1}V {:>8.3}A {:>10.2} kWh",
        name,
        reading.power_watts,
        reading.voltage.unwrap_or(0.0),
        reading.current.unwrap_or(0.0),
        reading.total_energy_wh / 1000.0,
    );
}

pub fn format_duration(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let mins = (seconds % 3600) / 60;

    if days > 0 {
        format!("{days}d {hours}h {mins}m")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

pub fn print_device_info(info: &DeviceInfo, status: &DeviceStatus) {
    let color = use_color();

    let name = info.display_name();
    if color {
        println!("{}", name.bold());
    } else {
        println!("{name}");
    }

    let label = |l: &str| -> String {
        if color {
            format!("{}", l.dimmed())
        } else {
            l.to_string()
        }
    };

    // Model line: friendly app name + model ID for Gen2/3, just model for Gen1
    let model_display = if let Some(ref app) = info.app {
        format!("{app} ({})", info.model)
    } else if let Some(ref dtype) = info.device_type {
        format!("Shelly ({dtype})")
    } else {
        info.model.clone()
    };
    println!("  {}  {model_display}", label("Model:      "));
    println!("  {}  {}", label("Generation: "), info.generation);
    println!("  {}  {}", label("IP:         "), info.ip);
    if !info.mac.is_empty() {
        println!("  {}  {}", label("MAC:        "), info.mac);
    }
    println!(
        "  {}  {}",
        label("Firmware:   "),
        short_fw(&info.firmware_version)
    );

    if let Some(uptime) = status.uptime {
        println!("  {}  {}", label("Uptime:     "), format_duration(uptime));
    }

    if let Some(ref wifi) = status.wifi {
        let rssi_str = wifi.rssi.map_or(String::new(), |r| format!(" ({r} dBm)"));
        let ssid = wifi.ssid.as_deref().unwrap_or("?");
        println!("  {}  {ssid}{rssi_str}", label("WiFi:       "));
    }

    if let Some(cloud) = status.cloud_connected {
        let cloud_str = if cloud { "connected" } else { "disconnected" };
        println!("  {}  {cloud_str}", label("Cloud:      "));
    }

    if let Some(temp) = status.temperature_c {
        println!("  {}  {temp:.1}\u{00b0}C", label("Temperature:"));
    }

    for sw in &status.switches {
        let state = if sw.output { "ON" } else { "OFF" };
        let power_str = sw
            .power_watts
            .map(|w| format!(" ({w:.1}W)"))
            .unwrap_or_default();

        let state_display = if color {
            if sw.output {
                format!("{}{power_str}", state.green())
            } else {
                format!("{}{power_str}", state.dimmed())
            }
        } else {
            format!("{state}{power_str}")
        };

        let switch_label = format!("Switch {}:   ", sw.id);
        println!("  {}  {state_display}", label(&switch_label));

        if let Some(total) = sw.total_energy_wh {
            let kwh = total / 1000.0;
            println!("  {}  {kwh:.2} kWh", label("Total Energy:"));
        }
    }
}

pub fn device_info_json(info: &DeviceInfo, status: &DeviceStatus) -> serde_json::Value {
    let model_display = if let Some(ref app) = info.app {
        format!("{app} ({})", info.model)
    } else if let Some(ref dtype) = info.device_type {
        format!("Shelly ({dtype})")
    } else {
        info.model.clone()
    };

    let cloud_str = status
        .cloud_connected
        .map(|c| if c { "connected" } else { "disconnected" });

    let wifi_ssid = status.wifi.as_ref().and_then(|w| w.ssid.clone());
    let wifi_rssi = status.wifi.as_ref().and_then(|w| w.rssi);

    let switches: Vec<serde_json::Value> = status
        .switches
        .iter()
        .map(|sw| {
            serde_json::json!({
                "id": sw.id,
                "output": sw.output,
                "power_watts": sw.power_watts,
                "total_energy_wh": sw.total_energy_wh,
            })
        })
        .collect();

    serde_json::json!({
        "name": info.display_name(),
        "model": model_display,
        "model_id": info.model,
        "generation": info.generation.to_string(),
        "ip": info.ip.to_string(),
        "mac": info.mac,
        "firmware": short_fw(&info.firmware_version),
        "uptime_seconds": status.uptime,
        "uptime": status.uptime.map(format_duration),
        "wifi_ssid": wifi_ssid,
        "wifi_rssi": wifi_rssi,
        "cloud": cloud_str,
        "temperature_c": status.temperature_c,
        "switches": switches,
    })
}

/// Width of the energy table (device name + total kWh columns).
const ENERGY_TABLE_WIDTH: usize = 46;

pub fn print_energy_header() {
    let header = format!("{:<34} {:>11}", "Device", "Total (kWh)");
    if use_color() {
        println!("{}", header.bold());
        println!("{}", "\u{2500}".repeat(ENERGY_TABLE_WIDTH).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(ENERGY_TABLE_WIDTH));
    }
}

pub fn print_energy_row(name: &str, kwh: Option<f64>) {
    match kwh {
        Some(val) => println!("{:<34} {:>11.2}", name, val),
        None => println!("{:<34} {:>11}", name, "-"),
    }
}

pub fn print_energy_footer(total_kwh: f64) {
    if use_color() {
        println!("{}", "\u{2500}".repeat(ENERGY_TABLE_WIDTH).dimmed());
        println!("{}", format!("{:<34} {:>11.2}", "Total", total_kwh).bold());
    } else {
        println!("{}", "-".repeat(ENERGY_TABLE_WIDTH));
        println!("{:<34} {:>11.2}", "Total", total_kwh);
    }
}

pub fn format_duration_short(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let mins = (seconds % 3600) / 60;

    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{mins}m")
    } else {
        format!("{mins}m")
    }
}
