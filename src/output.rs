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

pub fn print_device_table(devices: &[DeviceInfo]) {
    if devices.is_empty() {
        eprintln!("No devices found.");
        return;
    }

    let color = use_color();
    let header = format!(
        "{:<30} {:<16} {:<5} {:<12} {:<10} {:<18}",
        "Name", "IP", "Gen", "Model", "FW", "MAC"
    );
    if color {
        println!("{}", header.bold());
    } else {
        println!("{header}");
    }

    let sep = "-".repeat(95);
    if color {
        println!("{}", sep.dimmed());
    } else {
        println!("{sep}");
    }

    for d in devices {
        println!(
            "{:<30} {:<16} {:<5} {:<12} {:<10} {:<18}",
            d.display_name(),
            d.ip,
            d.generation,
            d.model,
            d.firmware_version,
            d.mac,
        );
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
        println!("  Cloud: {}", if cloud { "connected" } else { "disconnected" });
    }
    if let Some(mqtt) = status.mqtt_connected {
        println!("  MQTT: {}", if mqtt { "connected" } else { "disconnected" });
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
        if color { "ON".green().to_string() } else { "ON".to_string() }
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
