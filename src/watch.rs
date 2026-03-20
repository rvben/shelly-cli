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

/// A selectable row in the watch table, mapping to a device + switch.
struct SelectableRow {
    device_index: usize,
    switch_id: u8,
    is_online: bool,
    has_switch: bool,
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
    let mut selected: usize = 0;
    let mut status_msg: Option<(String, tokio::time::Instant)> = None;

    loop {
        let snapshots = poll_all(devices, client).await;
        let rows = build_selectable_rows(&snapshots);

        // Clamp selection
        let row_count = rows.len();
        if row_count > 0 && selected >= row_count {
            selected = row_count - 1;
        }

        render(stdout, &snapshots, selected, &status_msg)?;

        // Wait for interval or keypress
        let deadline = tokio::time::Instant::now() + interval;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            // Clear expired status messages
            if let Some((_, expires)) = &status_msg
                && tokio::time::Instant::now() >= *expires
            {
                status_msg = None;
                render(stdout, &snapshots, selected, &status_msg)?;
            }

            if event::poll(remaining.min(Duration::from_millis(100)))?
                && let Event::Key(key) = event::read()?
            {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if selected > 0 {
                            selected -= 1;
                            render(stdout, &snapshots, selected, &status_msg)?;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if row_count > 0 && selected < row_count - 1 {
                            selected += 1;
                            render(stdout, &snapshots, selected, &status_msg)?;
                        }
                    }
                    KeyCode::Home => {
                        selected = 0;
                        render(stdout, &snapshots, selected, &status_msg)?;
                    }
                    KeyCode::End => {
                        if row_count > 0 {
                            selected = row_count - 1;
                        }
                        render(stdout, &snapshots, selected, &status_msg)?;
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        if let Some(row) = rows.get(selected) {
                            if !row.is_online {
                                status_msg = Some((
                                    "device is offline".to_string(),
                                    tokio::time::Instant::now() + Duration::from_secs(3),
                                ));
                            } else if !row.has_switch {
                                status_msg = Some((
                                    "device has no switch".to_string(),
                                    tokio::time::Instant::now() + Duration::from_secs(3),
                                ));
                            } else {
                                let info = &devices[row.device_index];
                                let device = api::create_device(info.clone(), client.clone());
                                let switch_id = row.switch_id;
                                match device.switch_toggle(switch_id).await {
                                    Ok(result) => {
                                        let new_state = if result.was_on { "OFF" } else { "ON" };
                                        status_msg = Some((
                                            format!(
                                                "toggled {} → {}",
                                                info.display_name(),
                                                new_state
                                            ),
                                            tokio::time::Instant::now() + Duration::from_secs(3),
                                        ));
                                    }
                                    Err(e) => {
                                        status_msg = Some((
                                            format!("toggle failed: {e}"),
                                            tokio::time::Instant::now() + Duration::from_secs(5),
                                        ));
                                    }
                                }
                                // Break to refresh immediately after toggle
                                break;
                            }
                            render(stdout, &snapshots, selected, &status_msg)?;
                        }
                    }
                    KeyCode::Char(c @ '1'..='9') => {
                        let idx = (c as usize) - ('1' as usize);
                        if idx < row_count {
                            selected = idx;
                            render(stdout, &snapshots, selected, &status_msg)?;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn build_selectable_rows(snapshots: &[DeviceSnapshot]) -> Vec<SelectableRow> {
    let mut rows = Vec::new();
    for (device_index, snap) in snapshots.iter().enumerate() {
        if !snap.online {
            rows.push(SelectableRow {
                device_index,
                switch_id: 0,
                is_online: false,
                has_switch: false,
            });
        } else if snap.switches.is_empty() {
            rows.push(SelectableRow {
                device_index,
                switch_id: 0,
                is_online: true,
                has_switch: false,
            });
        } else {
            for sw in &snap.switches {
                rows.push(SelectableRow {
                    device_index,
                    switch_id: sw.id,
                    is_online: true,
                    has_switch: true,
                });
            }
        }
    }
    rows
}

async fn poll_all(devices: &[DeviceInfo], client: &reqwest::Client) -> Vec<DeviceSnapshot> {
    let mut snapshots = Vec::with_capacity(devices.len());

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

fn render(
    stdout: &mut io::Stdout,
    snapshots: &[DeviceSnapshot],
    selected: usize,
    status_msg: &Option<(String, tokio::time::Instant)>,
) -> Result<()> {
    execute!(
        stdout,
        cursor::MoveTo(0, 0),
        terminal::Clear(ClearType::All)
    )?;

    let now = chrono::Local::now().format("%H:%M:%S");
    writeln!(
        stdout,
        " {}  |  {now}  |  {} select  {} toggle  {} quit\r",
        "shelly watch".bold(),
        "↑↓".bold(),
        "⏎".bold(),
        "q".bold(),
    )?;

    // Show status message if active
    if let Some((msg, _)) = status_msg {
        writeln!(stdout, " {}\r", msg.yellow())?;
    } else {
        writeln!(stdout, "\r")?;
    }

    let header = format!(
        "   {:<30} {:<5} {:>8} {:>8} {:>7} {:>10} {:>6} Uptime",
        "Device", "State", "Power", "Voltage", "Temp", "Energy", "RSSI"
    );
    writeln!(stdout, "{}\r", header.bold())?;

    writeln!(stdout, "   {}\r", "-".repeat(93).dimmed())?;

    let mut total_power = 0.0;
    let mut on_count = 0u32;
    let mut total_count = 0u32;
    let mut online_count = 0u32;
    let mut row_idx = 0usize;

    for snap in snapshots {
        if !snap.online {
            let indicator = if row_idx == selected {
                ">".bold().cyan().to_string()
            } else {
                " ".to_string()
            };
            let line = format!(
                " {} {:<30} {:<5} {:>8} {:>8} {:>7} {:>10} {:>6} -",
                indicator,
                snap.name.red(),
                "OFFLINE".red().bold(),
                "-".dimmed(),
                "-".dimmed(),
                "-".dimmed(),
                "-".dimmed(),
                "-".dimmed()
            );
            if row_idx == selected {
                writeln!(stdout, "{}\r", line.on_bright_black())?;
            } else {
                writeln!(stdout, "{line}\r")?;
            }
            total_count += 1;
            row_idx += 1;
            continue;
        }

        online_count += 1;

        if snap.switches.is_empty() {
            let indicator = if row_idx == selected {
                ">".bold().cyan().to_string()
            } else {
                " ".to_string()
            };
            let temp = snap
                .temperature_c
                .map(|t| format!("{t:.0}°C"))
                .unwrap_or_else(|| "-".into());
            let rssi = snap
                .rssi
                .map(|r| format!("{r}"))
                .unwrap_or_else(|| "-".into());
            let uptime = snap
                .uptime
                .map(format_duration_short)
                .unwrap_or_else(|| "-".into());

            let line = format!(
                " {} {:<30} {:<5} {:>8} {:>8} {:>7} {:>10} {:>6} {}",
                indicator,
                snap.name,
                "-".dimmed(),
                "-".dimmed(),
                "-".dimmed(),
                temp,
                "-".dimmed(),
                rssi,
                uptime,
            );
            if row_idx == selected {
                writeln!(stdout, "{}\r", line.on_bright_black())?;
            } else {
                writeln!(stdout, "{line}\r")?;
            }
            total_count += 1;
            row_idx += 1;
        } else {
            for sw in &snap.switches {
                total_count += 1;
                let indicator = if row_idx == selected {
                    ">".bold().cyan().to_string()
                } else {
                    " ".to_string()
                };

                let label = if snap.switches.len() > 1 {
                    format!("{} [{}]", snap.name, sw.id)
                } else {
                    snap.name.clone()
                };

                let (state, state_padded): (String, String) = if sw.output {
                    on_count += 1;
                    let s = "ON".green().to_string();
                    (s.clone(), format!("{s}   "))
                } else {
                    let s = "OFF".dimmed().to_string();
                    (s.clone(), format!("{s}  "))
                };
                let _ = state; // used via state_padded

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
                    .map(|t| format!("{t:.0}°C"))
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

                let line = format!(
                    " {} {:<30} {} {:>8} {:>8} {:>7} {:>10} {:>6} {}",
                    indicator, label, state_padded, power, voltage, temp, energy, rssi, uptime,
                );
                if row_idx == selected {
                    writeln!(stdout, "{}\r", line.on_bright_black())?;
                } else {
                    writeln!(stdout, "{line}\r")?;
                }
                row_idx += 1;
            }
        }
    }

    writeln!(stdout, "   {}\r", "-".repeat(93).dimmed())?;

    let power_display = format!("{total_power:.1}W").bold().to_string();
    writeln!(
        stdout,
        "   Total: {power_display}  |  {on_count}/{total_count} ON  |  {online_count}/{} online\r",
        snapshots.len()
    )?;

    stdout.flush()?;
    Ok(())
}
