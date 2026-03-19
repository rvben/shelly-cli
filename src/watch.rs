use std::io::{self, Write};
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    cursor, execute,
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{self, ClearType},
};
use owo_colors::OwoColorize;

use crate::api;
use crate::model::DeviceInfo;
use crate::output::format_duration_short;

/// RAII guard that restores terminal state on drop, even if the watch loop panics.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), cursor::Show, terminal::LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

struct DeviceSnapshot {
    name: String,
    online: bool,
    switches: Vec<SwitchSnapshot>,
    temperature_c: Option<f64>,
    rssi: Option<i32>,
    uptime: Option<u64>,
}

struct SwitchSnapshot {
    id: u8,
    output: bool,
    power_watts: Option<f64>,
    voltage: Option<f64>,
    total_energy_wh: Option<f64>,
}

pub async fn run(
    devices: &[DeviceInfo],
    client: &reqwest::Client,
    interval: Duration,
) -> Result<()> {
    terminal::enable_raw_mode()?;
    let _guard = TerminalGuard;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

    watch_loop(devices, client, interval, &mut stdout).await
}

async fn watch_loop(
    devices: &[DeviceInfo],
    client: &reqwest::Client,
    interval: Duration,
    stdout: &mut io::Stdout,
) -> Result<()> {
    loop {
        let snapshots = poll_all(devices, client).await;
        render(stdout, &snapshots)?;

        // Wait for interval or keypress
        let deadline = tokio::time::Instant::now() + interval;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            // Poll for keyboard events with a short timeout
            if event::poll(remaining.min(Duration::from_millis(100)))?
                && let Event::Key(key) = event::read()?
            {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn poll_all(devices: &[DeviceInfo], client: &reqwest::Client) -> Vec<DeviceSnapshot> {
    let mut snapshots = Vec::with_capacity(devices.len());

    // Poll devices concurrently
    let handles: Vec<_> = devices
        .iter()
        .map(|info| {
            let device = api::create_device(info.clone(), client.clone());
            let name = info.display_name().to_string();

            tokio::spawn(async move {
                match device.status().await {
                    Ok(status) => DeviceSnapshot {
                        name,
                        online: true,
                        switches: status
                            .switches
                            .iter()
                            .map(|sw| SwitchSnapshot {
                                id: sw.id,
                                output: sw.output,
                                power_watts: sw.power_watts,
                                voltage: sw.voltage,
                                total_energy_wh: sw.total_energy_wh,
                            })
                            .collect(),
                        temperature_c: status.temperature_c,
                        rssi: status.wifi.as_ref().and_then(|w| w.rssi),
                        uptime: status.uptime,
                    },
                    Err(_) => DeviceSnapshot {
                        name,
                        online: false,
                        switches: Vec::new(),
                        temperature_c: None,
                        rssi: None,
                        uptime: None,
                    },
                }
            })
        })
        .collect();

    for handle in handles {
        if let Ok(snapshot) = handle.await {
            snapshots.push(snapshot);
        }
    }

    snapshots
}

fn render(stdout: &mut io::Stdout, snapshots: &[DeviceSnapshot]) -> Result<()> {
    // Watch mode always runs in a TTY (alternate screen), so color is always on
    execute!(
        stdout,
        cursor::MoveTo(0, 0),
        terminal::Clear(ClearType::All)
    )?;

    let now = chrono::Local::now().format("%H:%M:%S");
    writeln!(
        stdout,
        " {}  |  {now}  |  press {} to quit\r",
        "shelly watch".bold(),
        "q".bold()
    )?;
    writeln!(stdout, "\r")?;

    let header = format!(
        " {:<30} {:<5} {:>8} {:>8} {:>7} {:>10} {:>6} Uptime",
        "Device", "State", "Power", "Voltage", "Temp", "Energy", "RSSI"
    );
    writeln!(stdout, "{}\r", header.bold())?;

    writeln!(stdout, " {}\r", "-".repeat(95).dimmed())?;

    let mut total_power = 0.0;
    let mut on_count = 0u32;
    let mut total_count = 0u32;
    let mut online_count = 0u32;

    for snap in snapshots {
        if !snap.online {
            writeln!(
                stdout,
                " {:<30} {:<5} {:>8} {:>8} {:>7} {:>10} {:>6} -\r",
                snap.name.red(),
                "OFFLINE".red().bold(),
                "-".dimmed(),
                "-".dimmed(),
                "-".dimmed(),
                "-".dimmed(),
                "-".dimmed()
            )?;
            total_count += 1;
            continue;
        }

        online_count += 1;

        if snap.switches.is_empty() {
            let temp = snap
                .temperature_c
                .map(|t| format!("{t:.0}\u{00b0}C"))
                .unwrap_or_else(|| "-".into());
            let rssi = snap
                .rssi
                .map(|r| format!("{r}"))
                .unwrap_or_else(|| "-".into());
            let uptime = snap
                .uptime
                .map(format_duration_short)
                .unwrap_or_else(|| "-".into());

            writeln!(
                stdout,
                " {:<30} {:<5} {:>8} {:>8} {:>7} {:>10} {:>6} {}\r",
                snap.name,
                "-".dimmed(),
                "-".dimmed(),
                "-".dimmed(),
                temp,
                "-".dimmed(),
                rssi,
                uptime,
            )?;
            total_count += 1;
        } else {
            for sw in &snap.switches {
                total_count += 1;
                let label = if snap.switches.len() > 1 {
                    format!("{} [{}]", snap.name, sw.id)
                } else {
                    snap.name.clone()
                };

                let state: String = if sw.output {
                    on_count += 1;
                    "ON".green().to_string()
                } else {
                    "OFF".dimmed().to_string()
                };

                let power = sw
                    .power_watts
                    .map(|p| {
                        total_power += p;
                        format!("{p:.1}W")
                    })
                    .unwrap_or_else(|| "-".into());

                let voltage = sw
                    .voltage
                    .map(|v| format!("{v:.0}V"))
                    .unwrap_or_else(|| "-".into());

                let temp = snap
                    .temperature_c
                    .map(|t| format!("{t:.0}\u{00b0}C"))
                    .unwrap_or_else(|| "-".into());

                let energy = sw
                    .total_energy_wh
                    .map(|e| format!("{:.1}kWh", e / 1000.0))
                    .unwrap_or_else(|| "-".into());

                let rssi = snap
                    .rssi
                    .map(|r| format!("{r}"))
                    .unwrap_or_else(|| "-".into());

                let uptime = snap
                    .uptime
                    .map(format_duration_short)
                    .unwrap_or_else(|| "-".into());

                // ANSI codes break format width, so pad state manually
                let state_padded = if sw.output {
                    // "ON" is 2 chars, pad to 5
                    format!("{state}   ")
                } else {
                    // "OFF" is 3 chars, pad to 5
                    format!("{state}  ")
                };

                writeln!(
                    stdout,
                    " {:<30} {} {:>8} {:>8} {:>7} {:>10} {:>6} {}\r",
                    label, state_padded, power, voltage, temp, energy, rssi, uptime,
                )?;
            }
        }
    }

    writeln!(stdout, " {}\r", "-".repeat(95).dimmed())?;

    let power_display = format!("{total_power:.1}W").bold().to_string();
    writeln!(
        stdout,
        " Total: {power_display}  |  {on_count}/{total_count} ON  |  {online_count}/{} online\r",
        snapshots.len()
    )?;

    stdout.flush()?;
    Ok(())
}
