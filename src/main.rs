mod api;
mod cache;
mod cli;
mod config;
mod errors;
mod groups;
mod health;
mod model;
mod output;
mod schema;
mod watch;

use std::io::IsTerminal;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{CommandFactory, FromArgMatches};
use owo_colors::OwoColorize;

use cli::{
    Cli, Command, ConfigAction, FirmwareAction, GroupAction, ScheduleAction, SwitchAction,
    WebhookAction,
};
use model::DeviceInfo;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        let cli_error = errors::classify_error(&err);
        let exit_code = cli_error.code.exit_code();

        let json_mode = std::env::args().any(|a| a == "--json" || a == "-j")
            || !std::io::stdout().is_terminal();

        if json_mode {
            output::print_json_error(&cli_error);
        } else {
            eprintln!("Error: {}", cli_error.message);
        }

        std::process::exit(exit_code);
    }
}

async fn run() -> Result<()> {
    let bin_name: &'static str = {
        let name = std::env::args()
            .next()
            .and_then(|arg| {
                std::path::Path::new(&arg)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "shelly".to_string());
        Box::leak(name.into_boxed_str())
    };

    let matches = Cli::command().name(bin_name).get_matches();
    let mut cli = Cli::from_arg_matches(&matches)?;

    let json_output = cli.json || !std::io::stdout().is_terminal();
    let timeout = Duration::from_millis(cli.timeout);

    let app_config = config::load_config()?;
    let password = cli.password.clone().or(app_config.auth.password);

    let http_client = reqwest::Client::builder().timeout(timeout).build()?;

    // Convert top-level On/Off/Toggle: extract positional device name and delegate to cmd_switch
    let shortcut_action = match &cli.command {
        Command::On { device, id } => Some((device.clone(), SwitchAction::On { id: *id })),
        Command::Off { device, id } => Some((device.clone(), SwitchAction::Off { id: *id })),
        Command::Toggle { device, id } => Some((device.clone(), SwitchAction::Toggle { id: *id })),
        _ => None,
    };
    if let Some((device, action)) = shortcut_action {
        if let Some(dev) = device {
            cli.name = cli.name.or(Some(dev));
        }
        return cmd_switch(&cli, &http_client, &password, action, json_output).await;
    }

    match cli.command {
        Command::Discover { subnet } => cmd_discover(subnet, timeout, json_output, cli.quiet).await,
        Command::Devices { refresh } => cmd_devices(refresh, timeout, json_output, cli.quiet).await,
        Command::Status { all } => {
            cmd_status(&cli, &http_client, &password, all, json_output).await
        }
        Command::Switch { ref action } => {
            cmd_switch(&cli, &http_client, &password, action.clone(), json_output).await
        }
        Command::Power { all, id } => {
            cmd_power(&cli, &http_client, &password, all, id, json_output).await
        }
        Command::Energy { all } => {
            cmd_energy(&cli, &http_client, &password, all, json_output).await
        }
        Command::Firmware { ref action } => {
            cmd_firmware(&cli, &http_client, &password, action.clone(), json_output).await
        }
        Command::Config { ref action } => {
            cmd_config(&cli, &http_client, &password, action.clone(), json_output).await
        }
        Command::Schedule { ref action } => {
            cmd_schedule(&cli, &http_client, &password, action.clone(), json_output).await
        }
        Command::Webhook { ref action } => {
            cmd_webhook(&cli, &http_client, &password, action.clone(), json_output).await
        }
        Command::Backup { all, ref output } => {
            cmd_backup(
                &cli,
                &http_client,
                &password,
                all,
                output.clone(),
                json_output,
            )
            .await
        }
        Command::Restore { ref file } => {
            cmd_restore(&cli, &http_client, &password, file, json_output).await
        }
        Command::Rename { ref new_name } => {
            cmd_rename(&cli, &http_client, &password, new_name, json_output).await
        }
        Command::Reboot => cmd_reboot(&cli, &http_client, &password, json_output).await,
        Command::Watch { interval } => cmd_watch(&cli, &http_client, &password, interval).await,
        Command::Info => cmd_info(&cli, &http_client, &password, json_output).await,
        Command::Health => cmd_health(&cli, &http_client, &password, json_output).await,
        Command::Group { ref action } => cmd_group(action.clone(), json_output),
        Command::Schema => {
            let schema = schema::generate_schema();
            println!("{}", serde_json::to_string_pretty(&schema)?);
            Ok(())
        }
        Command::Completions { shell } => {
            generate_completions(shell);
            Ok(())
        }
        Command::CompleteDeviceNames => {
            if let Ok(devices) = cache::load_devices() {
                for d in &devices {
                    println!("{}", d.display_name());
                }
            }
            Ok(())
        }
        Command::CompleteGroupNames => {
            if let Ok(groups) = groups::load_groups() {
                for name in groups.keys() {
                    println!("{name}");
                }
            }
            Ok(())
        }
        // Already handled above
        Command::On { .. } | Command::Off { .. } | Command::Toggle { .. } => unreachable!(),
    }
}

/// Resolve target devices from --host, --name, --group, or --all flags.
/// Returns a list of DeviceInfo to operate on.
fn resolve_targets(cli: &Cli) -> Result<Vec<DeviceInfo>> {
    if let Some(ref group_name) = cli.group {
        return groups::resolve_group(group_name);
    }

    if let Some(ref host) = cli.host {
        let ip: IpAddr = host
            .parse()
            .with_context(|| format!("invalid IP address: {host}"))?;

        let info = DeviceInfo {
            ip,
            name: None,
            id: String::new(),
            mac: String::new(),
            model: String::new(),
            generation: model::DeviceGeneration::Gen2,
            firmware_version: String::new(),
            auth_enabled: false,
            num_outputs: 1,
            num_meters: 1,
            app: None,
            device_type: None,
        };
        return Ok(vec![info]);
    }

    if let Some(ref name) = cli.name {
        let devices = cache::load_devices()?;
        let info = cache::find_device_by_name_with_suggestions(&devices, name)?;
        return Ok(vec![info]);
    }

    anyhow::bail!("specify --host <IP>, --name <NAME>, or --group <GROUP> to target device(s)")
}

/// Resolve targets and probe any that need it (e.g. --host without cached info).
async fn resolve_and_probe_targets(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
) -> Result<Vec<api::ShellyDevice>> {
    let infos = resolve_targets(cli)?;
    let mut devices = Vec::with_capacity(infos.len());

    for info in infos {
        let info = if info.id.is_empty() {
            api::probe_device(info.ip, http_client).await?
        } else {
            info
        };
        warn_if_auth_required(&info, password);
        devices.push(api::create_device(
            info,
            http_client.clone(),
            password.clone(),
        ));
    }

    Ok(devices)
}

/// Print a warning when a device requires authentication but no password was provided.
fn warn_if_auth_required(info: &DeviceInfo, password: &Option<String>) {
    if info.auth_enabled && password.is_none() {
        eprintln!(
            "Warning: {} ({}) has authentication enabled but no password provided. \
             Use --password or set [auth] password in config.toml.",
            info.display_name(),
            info.ip,
        );
    }
}

/// Load all cached devices, or resolve --group if specified.
fn resolve_all_or_group(cli: &Cli) -> Result<Vec<DeviceInfo>> {
    if let Some(ref group_name) = cli.group {
        return groups::resolve_group(group_name);
    }
    let devices = cache::load_devices()?;
    if devices.is_empty() {
        if cache::cache_exists() {
            anyhow::bail!(
                "Device cache is empty. Re-scan with:\n  shelly discover --subnet YOUR_SUBNET/24"
            );
        } else {
            anyhow::bail!(
                "No devices discovered yet. Get started with:\n  shelly discover --subnet YOUR_SUBNET/24"
            );
        }
    }
    Ok(devices)
}

fn colored_on_off(on: bool, color: bool) -> String {
    let color = color && output::use_color();
    if on {
        if color {
            "ON".green().to_string()
        } else {
            "ON".to_string()
        }
    } else if color {
        "OFF".dimmed().to_string()
    } else {
        "OFF".to_string()
    }
}

/// Auto-detect the local IPv4 subnet from network interfaces.
///
/// Prefers non-tunnel interfaces with private IPv4 addresses and reasonable
/// prefix lengths (/8 to /30). Falls back to the default interface if no
/// better candidate is found.
fn detect_subnet() -> Option<String> {
    let interfaces = netdev::get_interfaces();

    // Find the best candidate: non-tunnel, private IPv4, reasonable prefix
    let candidate = interfaces
        .iter()
        .filter(|iface| {
            // Skip loopback and tunnel interfaces (utun, tun, tap, wg)
            let name = &iface.name;
            !name.starts_with("lo")
                && !name.starts_with("utun")
                && !name.starts_with("tun")
                && !name.starts_with("tap")
                && !name.starts_with("wg")
                && !name.starts_with("tailscale")
                && !name.starts_with("docker")
                && !name.starts_with("br-")
                && !name.starts_with("veth")
        })
        .flat_map(|iface| &iface.ipv4)
        .find(|addr_info| {
            let ip = addr_info.addr();
            let prefix = addr_info.prefix_len();
            // Private ranges with reasonable subnet sizes
            ip.is_private() && (8..=30).contains(&prefix)
        });

    // Fall back to default interface
    let addr_info = if let Some(addr) = candidate {
        *addr
    } else {
        let iface = netdev::get_default_interface().ok()?;
        *iface.ipv4.first()?
    };

    let ip = addr_info.addr();
    let prefix_len = addr_info.prefix_len();

    // Compute the network address by masking the host bits
    let mask = if prefix_len >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - prefix_len)
    };
    let network_bits = u32::from(ip) & mask;
    let network_addr = Ipv4Addr::from(network_bits);

    Some(format!("{network_addr}/{prefix_len}"))
}

async fn cmd_discover(
    subnet_arg: Option<String>,
    timeout: Duration,
    json_output: bool,
    quiet: bool,
) -> Result<()> {
    let subnet_str = if let Some(ref s) = subnet_arg {
        s.clone()
    } else if let Some(detected) = detect_subnet() {
        if !quiet {
            eprintln!("Auto-detected subnet: {detected}");
        }
        detected
    } else {
        let app_config = config::load_config()?;
        app_config.network.subnet.clone()
    };

    let subnet: ipnet::Ipv4Net = subnet_str
        .parse()
        .with_context(|| format!("invalid subnet: {subnet_str}"))?;

    let show_progress = !quiet && !json_output && std::io::stderr().is_terminal();

    if !quiet && !show_progress {
        eprintln!("Scanning {subnet}...");
    }

    let mut devices = api::discovery::scan_subnet(subnet, timeout, show_progress, |info| {
        if !quiet && !json_output {
            if show_progress {
                // Clear progress line before printing found device
                eprint!("\r{}\r", " ".repeat(60));
            }
            eprintln!("  Found: {} at {}", info.display_name(), info.ip);
        }
    })
    .await?;

    let enrich_client = reqwest::Client::builder().timeout(timeout).build()?;

    for device in &mut devices {
        if device.name.is_none() {
            let _ = api::discovery::enrich_gen1_name(device, &enrich_client).await;
        }
    }

    cache::save_devices(&devices)?;

    if !quiet && !json_output {
        eprintln!("Found {} device(s), saved to cache.\n", devices.len());
    }

    if json_output {
        output::print_json_success(&devices);
    } else {
        output::print_device_table(&devices);
        if !devices.is_empty() {
            println!();
            println!("Found {} device(s). Try:", devices.len());
            println!("  shelly status -a");
            println!("  shelly health");
            println!("  shelly watch");
        }
    }

    Ok(())
}

async fn cmd_devices(
    refresh: bool,
    timeout: Duration,
    json_output: bool,
    quiet: bool,
) -> Result<()> {
    if refresh {
        return cmd_discover(None, timeout, json_output, quiet).await;
    }

    let devices = cache::load_devices()?;

    if devices.is_empty() {
        eprintln!("No cached devices. Run 'shelly discover' first.");
        return Ok(());
    }

    if json_output {
        output::print_json_success(&devices);
    } else {
        output::print_device_table(&devices);
    }

    Ok(())
}

async fn cmd_status(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    all: bool,
    json_output: bool,
) -> Result<()> {
    if all || cli.group.is_some() {
        let devices = resolve_all_or_group(cli)?;

        let mut results = Vec::new();
        let mut table_printed_header = false;

        for info in &devices {
            warn_if_auth_required(info, password);
            let device = api::create_device(info.clone(), http_client.clone(), password.clone());
            match device.status().await {
                Ok(status) => {
                    if json_output {
                        results.push(serde_json::json!({
                            "device": info.display_name(),
                            "ip": info.ip.to_string(),
                            "status": status,
                        }));
                    } else {
                        if !table_printed_header {
                            output::print_status_table_header();
                            table_printed_header = true;
                        }
                        output::print_status_table_row(
                            info.display_name(),
                            &info.ip.to_string(),
                            &status,
                        );
                    }
                }
                Err(e) => {
                    if json_output {
                        results.push(serde_json::json!({
                            "device": info.display_name(),
                            "ip": info.ip.to_string(),
                            "error": e.to_string(),
                        }));
                    } else {
                        if !table_printed_header {
                            output::print_status_table_header();
                            table_printed_header = true;
                        }
                        output::print_status_table_error(
                            info.display_name(),
                            &info.ip.to_string(),
                            &e.to_string(),
                        );
                    }
                }
            }
        }

        if json_output {
            output::print_json_success(&results);
        }
    } else {
        let targets = resolve_and_probe_targets(cli, http_client, password).await?;
        let device = &targets[0];
        let status = device.status().await?;

        if json_output {
            output::print_json_success(&status);
        } else {
            output::print_status(device.info().display_name(), &status);
        }
    }

    Ok(())
}

async fn cmd_switch(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    action: SwitchAction,
    json_output: bool,
) -> Result<()> {
    let targets = resolve_and_probe_targets(cli, http_client, password).await?;

    let mut json_results: Vec<serde_json::Value> = Vec::new();

    for device in &targets {
        let name = device.info().display_name().to_string();
        let switch_id = match action {
            SwitchAction::Status { id }
            | SwitchAction::On { id }
            | SwitchAction::Off { id }
            | SwitchAction::Toggle { id } => id,
        };
        validate_switch_id(device.info(), switch_id)?;

        match action {
            SwitchAction::Status { id } => {
                let status = device.switch_status(id).await?;
                if json_output {
                    json_results.push(serde_json::json!({
                        "device": name,
                        "status": status,
                    }));
                } else {
                    if targets.len() > 1 {
                        print!("{name}: ");
                    }
                    output::print_switch_status(&status);
                }
            }
            SwitchAction::On { id } => {
                let result = device.switch_set(id, true).await?;
                if json_output {
                    json_results
                        .push(serde_json::json!({ "device": name, "was_on": result.was_on }));
                } else {
                    let on_label = colored_on_off(true, !json_output);
                    let was_label = colored_on_off(result.was_on, !json_output);
                    println!("{name}: Switch {id} {on_label} (was {was_label})");
                }
            }
            SwitchAction::Off { id } => {
                let result = device.switch_set(id, false).await?;
                if json_output {
                    json_results
                        .push(serde_json::json!({ "device": name, "was_on": result.was_on }));
                } else {
                    let off_label = colored_on_off(false, !json_output);
                    let was_label = colored_on_off(result.was_on, !json_output);
                    println!("{name}: Switch {id} {off_label} (was {was_label})");
                }
            }
            SwitchAction::Toggle { id } => {
                let result = device.switch_toggle(id).await?;
                if json_output {
                    json_results
                        .push(serde_json::json!({ "device": name, "was_on": result.was_on }));
                } else {
                    let was_label = colored_on_off(result.was_on, !json_output);
                    let toggled = if output::use_color() {
                        "TOGGLED".cyan().to_string()
                    } else {
                        "TOGGLED".to_string()
                    };
                    println!("{name}: Switch {id} {toggled} (was {was_label})");
                }
            }
        }
    }

    if json_output {
        output::print_json_success(&json_results);
    }

    Ok(())
}

async fn cmd_power(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    all: bool,
    id: u8,
    json_output: bool,
) -> Result<()> {
    if all || cli.group.is_some() {
        let devices = resolve_all_or_group(cli)?;

        if !json_output {
            let header = format!(
                "{:<30} {:>8} {:>7} {:>8} {:>12}",
                "Device", "Power", "Volt", "Current", "Total"
            );
            if output::use_color() {
                println!("{}", header.bold());
                println!("{}", "-".repeat(70).dimmed());
            } else {
                println!("{header}");
                println!("{}", "-".repeat(70));
            }
        }

        let mut results = Vec::new();
        for info in &devices {
            warn_if_auth_required(info, password);
            let device = api::create_device(info.clone(), http_client.clone(), password.clone());
            for meter_id in 0..info.num_meters {
                let label = if info.num_meters > 1 {
                    format!("{} [{}]", info.display_name(), meter_id)
                } else {
                    info.display_name().to_string()
                };
                match device.power(meter_id).await {
                    Ok(reading) => {
                        if json_output {
                            results.push(serde_json::json!({
                                "device": info.display_name(),
                                "ip": info.ip.to_string(),
                                "meter_id": meter_id,
                                "power": reading,
                            }));
                        } else {
                            output::print_power_reading(&label, &reading);
                        }
                    }
                    Err(e) => {
                        if json_output {
                            results.push(serde_json::json!({
                                "device": info.display_name(),
                                "ip": info.ip.to_string(),
                                "meter_id": meter_id,
                                "error": e.to_string(),
                            }));
                        } else {
                            eprintln!("{:<30} error: {e}", label);
                        }
                    }
                }
            }
        }

        if json_output {
            output::print_json_success(&results);
        }
    } else {
        let targets = resolve_and_probe_targets(cli, http_client, password).await?;
        let device = &targets[0];
        validate_meter_id(device.info(), id)?;
        let reading = device.power(id).await?;

        if json_output {
            output::print_json_success(&reading);
        } else {
            output::print_power_reading(device.info().display_name(), &reading);
        }
    }

    Ok(())
}

async fn cmd_energy(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    all: bool,
    json_output: bool,
) -> Result<()> {
    if all || cli.group.is_some() {
        let devices = resolve_all_or_group(cli)?;

        if !json_output {
            output::print_energy_header();
        }

        let mut results = Vec::new();
        let mut grand_total_kwh = 0.0;

        for info in &devices {
            warn_if_auth_required(info, password);
            let device = api::create_device(info.clone(), http_client.clone(), password.clone());
            let name = info.display_name().to_string();

            if info.num_meters == 0 {
                if json_output {
                    results.push(serde_json::json!({
                        "device": name,
                        "ip": info.ip.to_string(),
                        "total_kwh": null,
                        "note": "no meter",
                    }));
                } else {
                    output::print_energy_row(&name, None);
                }
                continue;
            }

            let mut device_total_wh = 0.0;
            let mut any_error = false;

            for meter_id in 0..info.num_meters {
                match device.power(meter_id).await {
                    Ok(reading) => {
                        device_total_wh += reading.total_energy_wh;
                    }
                    Err(e) => {
                        any_error = true;
                        if json_output {
                            results.push(serde_json::json!({
                                "device": name,
                                "ip": info.ip.to_string(),
                                "error": e.to_string(),
                            }));
                        } else {
                            eprintln!("{:<34} error: {e}", name);
                        }
                    }
                }
            }

            if !any_error {
                let kwh = device_total_wh / 1000.0;
                grand_total_kwh += kwh;
                if json_output {
                    results.push(serde_json::json!({
                        "device": name,
                        "ip": info.ip.to_string(),
                        "total_kwh": kwh,
                    }));
                } else {
                    output::print_energy_row(&name, Some(kwh));
                }
            }
        }

        if json_output {
            output::print_json_success(&serde_json::json!({
                "devices": results,
                "total_kwh": grand_total_kwh,
            }));
        } else {
            output::print_energy_footer(grand_total_kwh);
        }
    } else {
        let targets = resolve_and_probe_targets(cli, http_client, password).await?;
        let device = &targets[0];
        let info = device.info();
        let name = info.display_name().to_string();

        let mut results = Vec::new();
        let mut device_total_wh = 0.0;

        for meter_id in 0..info.num_meters {
            let reading = device.power(meter_id).await?;
            device_total_wh += reading.total_energy_wh;
            results.push(reading);
        }

        let total_kwh = device_total_wh / 1000.0;

        if json_output {
            output::print_json_success(&serde_json::json!({
                "device": name,
                "total_kwh": total_kwh,
                "meters": results,
            }));
        } else if results.len() > 1 {
            output::print_energy_header();
            for reading in &results {
                let label = format!("{name} [{}]", reading.id);
                output::print_energy_row(&label, Some(reading.total_energy_wh / 1000.0));
            }
            output::print_energy_footer(total_kwh);
        } else {
            println!("{name}: {total_kwh:.2} kWh");
        }
    }

    Ok(())
}

/// Validate that a switch ID is within the device's output range.
fn validate_switch_id(info: &DeviceInfo, id: u8) -> Result<()> {
    if id >= info.num_outputs {
        anyhow::bail!(
            "switch ID {id} is out of range for {} (has {num} output{s}; valid IDs: 0..{max})",
            info.display_name(),
            num = info.num_outputs,
            s = if info.num_outputs == 1 { "" } else { "s" },
            max = info.num_outputs - 1,
        );
    }
    Ok(())
}

/// Validate that a meter ID is within the device's meter range.
fn validate_meter_id(info: &DeviceInfo, id: u8) -> Result<()> {
    if id >= info.num_meters {
        anyhow::bail!(
            "meter ID {id} is out of range for {} (has {num} meter{s}; valid IDs: 0..{max})",
            info.display_name(),
            num = info.num_meters,
            s = if info.num_meters == 1 { "" } else { "s" },
            max = info.num_meters - 1,
        );
    }
    Ok(())
}

async fn cmd_firmware(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    action: FirmwareAction,
    json_output: bool,
) -> Result<()> {
    match action {
        FirmwareAction::Check { all } => {
            if all || cli.group.is_some() {
                let devices = resolve_all_or_group(cli)?;

                if !json_output {
                    let header = format!(
                        "{:<30} {:<16} {:<12} {:<12} {:<20}",
                        "Device", "IP", "Current", "Stable", "Beta"
                    );
                    if output::use_color() {
                        println!("{}", header.bold());
                        println!("{}", "-".repeat(90).dimmed());
                    } else {
                        println!("{header}");
                        println!("{}", "-".repeat(90));
                    }
                }

                let mut results = Vec::new();
                for info in &devices {
                    warn_if_auth_required(info, password);
                    let device =
                        api::create_device(info.clone(), http_client.clone(), password.clone());
                    match device.firmware_check().await {
                        Ok(fw) => {
                            if json_output {
                                results.push(serde_json::json!({
                                    "device": info.display_name(),
                                    "ip": info.ip.to_string(),
                                    "firmware": fw.current_version,
                                    "has_update": fw.has_update,
                                    "stable": fw.stable_version,
                                    "beta": fw.beta_version,
                                }));
                            } else {
                                let current = output::short_fw(&fw.current_version);
                                let stable_str = fw
                                    .stable_version
                                    .as_deref()
                                    .map(output::short_fw)
                                    .unwrap_or("-");
                                let beta_str = fw
                                    .beta_version
                                    .as_deref()
                                    .map(output::short_fw)
                                    .unwrap_or("-");

                                if output::use_color() {
                                    if fw.has_update {
                                        println!(
                                            "{:<30} {:<16} {:<12} {:<12} {}",
                                            info.display_name().yellow(),
                                            info.ip,
                                            current,
                                            stable_str.green(),
                                            beta_str,
                                        );
                                    } else {
                                        println!(
                                            "{:<30} {:<16} {:<12} {:<12} {}",
                                            info.display_name(),
                                            info.ip,
                                            current.green(),
                                            stable_str.dimmed(),
                                            beta_str.dimmed(),
                                        );
                                    }
                                } else {
                                    let update_marker = if fw.has_update { " *" } else { "" };
                                    println!(
                                        "{:<30} {:<16} {:<12} {:<12} {:<20}",
                                        info.display_name(),
                                        info.ip,
                                        current,
                                        stable_str,
                                        format!("{beta_str}{update_marker}"),
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            if json_output {
                                results.push(serde_json::json!({
                                    "device": info.display_name(),
                                    "ip": info.ip.to_string(),
                                    "error": e.to_string(),
                                }));
                            } else {
                                eprintln!("{:<30} error: {e}", info.display_name());
                            }
                        }
                    }
                }

                if json_output {
                    output::print_json_success(&results);
                }
            } else {
                let targets = resolve_and_probe_targets(cli, http_client, password).await?;
                let device = &targets[0];
                let fw = device.firmware_check().await?;

                if json_output {
                    output::print_json_success(&serde_json::json!({
                        "current_version": fw.current_version,
                        "has_update": fw.has_update,
                        "stable_version": fw.stable_version,
                        "beta_version": fw.beta_version,
                    }));
                } else {
                    println!("Current: {}", fw.current_version);
                    println!("Update available: {}", fw.has_update);
                    if let Some(stable) = &fw.stable_version {
                        println!("Stable: {stable}");
                    }
                    if let Some(beta) = &fw.beta_version {
                        println!("Beta: {beta}");
                    }
                }
            }
        }
        FirmwareAction::Update { all } => {
            let infos = if all || cli.group.is_some() {
                resolve_all_or_group(cli)?
            } else {
                resolve_targets(cli)?
            };

            let mut results = Vec::new();
            for info in &infos {
                warn_if_auth_required(info, password);
                let device =
                    api::create_device(info.clone(), http_client.clone(), password.clone());
                let name = info.display_name();

                match device.firmware_check().await {
                    Ok(fw) if fw.has_update => {
                        if !json_output {
                            eprint!("{name}: updating from {}...", fw.current_version);
                        }
                        match device.firmware_update().await {
                            Ok(()) => {
                                if json_output {
                                    results.push(serde_json::json!({
                                        "device": name,
                                        "ip": info.ip.to_string(),
                                        "status": "updating",
                                        "from": fw.current_version,
                                        "to": fw.stable_version,
                                    }));
                                } else {
                                    eprintln!(
                                        " update triggered (-> {})",
                                        fw.stable_version.as_deref().unwrap_or("latest")
                                    );
                                }
                            }
                            Err(e) => {
                                if json_output {
                                    results.push(serde_json::json!({
                                        "device": name,
                                        "ip": info.ip.to_string(),
                                        "error": e.to_string(),
                                    }));
                                } else {
                                    eprintln!(" failed: {e}");
                                }
                            }
                        }
                    }
                    Ok(_) => {
                        if json_output {
                            results.push(serde_json::json!({
                                "device": name,
                                "ip": info.ip.to_string(),
                                "status": "up_to_date",
                            }));
                        } else {
                            println!("{name}: already up to date");
                        }
                    }
                    Err(e) => {
                        if json_output {
                            results.push(serde_json::json!({
                                "device": name,
                                "ip": info.ip.to_string(),
                                "error": e.to_string(),
                            }));
                        } else {
                            eprintln!("{name}: error checking firmware: {e}");
                        }
                    }
                }
            }

            if json_output {
                output::print_json_success(&results);
            }
        }
    }

    Ok(())
}

async fn cmd_config(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    action: ConfigAction,
    json_output: bool,
) -> Result<()> {
    match action {
        ConfigAction::Get { all } => {
            if all || cli.group.is_some() {
                let devices = resolve_all_or_group(cli)?;
                let mut results = Vec::new();
                for info in &devices {
                    warn_if_auth_required(info, password);
                    let device =
                        api::create_device(info.clone(), http_client.clone(), password.clone());
                    match device.config_get().await {
                        Ok(config) => {
                            results.push(serde_json::json!({
                                "device": info.display_name(),
                                "ip": info.ip.to_string(),
                                "config": config,
                            }));
                        }
                        Err(e) => {
                            results.push(serde_json::json!({
                                "device": info.display_name(),
                                "ip": info.ip.to_string(),
                                "error": e.to_string(),
                            }));
                        }
                    }
                }
                output::print_json_success(&results);
            } else {
                let targets = resolve_and_probe_targets(cli, http_client, password).await?;
                let device = &targets[0];
                let config = device.config_get().await?;
                output::print_json_success(&config);
            }
        }
        ConfigAction::Set { key, value } => {
            let targets = resolve_and_probe_targets(cli, http_client, password).await?;
            let device = &targets[0];
            device.config_set(&key, &value).await?;
            if json_output {
                output::print_json_success(&serde_json::json!({
                    "device": device.info().display_name(),
                    "key": key,
                    "value": value,
                    "status": "applied",
                }));
            } else {
                println!("{}: set {} = {}", device.info().display_name(), key, value);
            }
        }
    }

    Ok(())
}

async fn cmd_schedule(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    action: ScheduleAction,
    json_output: bool,
) -> Result<()> {
    match action {
        ScheduleAction::List { all } => {
            if all || cli.group.is_some() {
                let devices = resolve_all_or_group(cli)?;
                let mut results = Vec::new();
                for info in &devices {
                    warn_if_auth_required(info, password);
                    let device =
                        api::create_device(info.clone(), http_client.clone(), password.clone());
                    match device.schedule_list().await {
                        Ok(schedules) => {
                            results.push(serde_json::json!({
                                "device": info.display_name(),
                                "ip": info.ip.to_string(),
                                "schedules": schedules,
                            }));
                        }
                        Err(e) => {
                            if json_output {
                                results.push(serde_json::json!({
                                    "device": info.display_name(),
                                    "ip": info.ip.to_string(),
                                    "error": e.to_string(),
                                }));
                            } else {
                                eprintln!("{}: {e}", info.display_name());
                            }
                        }
                    }
                }
                if json_output {
                    output::print_json_success(&results);
                } else {
                    for result in &results {
                        let name = result["device"].as_str().unwrap_or("?");
                        let schedules = result["schedules"].as_array();
                        if let Some(scheds) = schedules {
                            if scheds.is_empty() {
                                println!("{name}: no schedules");
                            } else {
                                println!("{name}: {} schedule(s)", scheds.len());
                                for s in scheds {
                                    let id = s.get("id").and_then(|v| v.as_i64()).unwrap_or(-1);
                                    let enabled =
                                        s.get("enable").and_then(|v| v.as_bool()).unwrap_or(false);
                                    let timespec =
                                        s.get("timespec").and_then(|v| v.as_str()).unwrap_or("?");
                                    let status = if enabled { "enabled" } else { "disabled" };
                                    println!("  [{id}] {timespec} ({status})");
                                }
                            }
                        }
                    }
                }
            } else {
                let targets = resolve_and_probe_targets(cli, http_client, password).await?;
                let device = &targets[0];
                let schedules = device.schedule_list().await?;
                if json_output {
                    output::print_json_success(&schedules);
                } else {
                    let arr = schedules.as_array();
                    if arr.is_none_or(|a| a.is_empty()) {
                        println!("{}: no schedules", device.info().display_name());
                    } else {
                        for s in arr.unwrap() {
                            let id = s.get("id").and_then(|v| v.as_i64()).unwrap_or(-1);
                            let enabled =
                                s.get("enable").and_then(|v| v.as_bool()).unwrap_or(false);
                            let timespec =
                                s.get("timespec").and_then(|v| v.as_str()).unwrap_or("?");
                            let status = if enabled { "enabled" } else { "disabled" };
                            println!("  [{id}] {timespec} ({status})");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn cmd_webhook(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    action: WebhookAction,
    json_output: bool,
) -> Result<()> {
    match action {
        WebhookAction::List { all } => {
            if all || cli.group.is_some() {
                let devices = resolve_all_or_group(cli)?;
                let mut results = Vec::new();
                for info in &devices {
                    warn_if_auth_required(info, password);
                    let device =
                        api::create_device(info.clone(), http_client.clone(), password.clone());
                    match device.webhook_list().await {
                        Ok(hooks) => {
                            results.push(serde_json::json!({
                                "device": info.display_name(),
                                "ip": info.ip.to_string(),
                                "webhooks": hooks,
                            }));
                        }
                        Err(e) => {
                            if json_output {
                                results.push(serde_json::json!({
                                    "device": info.display_name(),
                                    "ip": info.ip.to_string(),
                                    "error": e.to_string(),
                                }));
                            } else {
                                eprintln!("{}: {e}", info.display_name());
                            }
                        }
                    }
                }
                if json_output {
                    output::print_json_success(&results);
                } else {
                    for result in &results {
                        let name = result["device"].as_str().unwrap_or("?");
                        let hooks = result["webhooks"].as_array();
                        if let Some(hooks) = hooks {
                            if hooks.is_empty() {
                                println!("{name}: no webhooks");
                            } else {
                                println!("{name}: {} webhook(s)", hooks.len());
                                for h in hooks {
                                    print_webhook_entry(h);
                                }
                            }
                        }
                    }
                }
            } else {
                let targets = resolve_and_probe_targets(cli, http_client, password).await?;
                let device = &targets[0];
                let hooks = device.webhook_list().await?;
                if json_output {
                    output::print_json_success(&hooks);
                } else {
                    let arr = hooks.as_array();
                    if arr.is_none_or(|a| a.is_empty()) {
                        println!("{}: no webhooks", device.info().display_name());
                    } else {
                        for h in arr.unwrap() {
                            print_webhook_entry(h);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn print_webhook_entry(h: &serde_json::Value) {
    // Gen2 format
    if let Some(id) = h.get("id").and_then(|v| v.as_i64()) {
        let enabled = h.get("enable").and_then(|v| v.as_bool()).unwrap_or(false);
        let event = h.get("event").and_then(|v| v.as_str()).unwrap_or("?");
        let name = h.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let status = if enabled { "enabled" } else { "disabled" };
        println!("  [{id}] {name} on {event} ({status})");
        if let Some(urls) = h.get("urls").and_then(|v| v.as_array()) {
            for url in urls {
                if let Some(u) = url.as_str() {
                    println!("       -> {u}");
                }
            }
        }
    }
}

async fn cmd_backup(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    all: bool,
    output_dir: Option<String>,
    json_output: bool,
) -> Result<()> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    if all || cli.group.is_some() {
        let devices = resolve_all_or_group(cli)?;
        let dir = output_dir.unwrap_or_else(|| "shelly-backups".to_string());
        std::fs::create_dir_all(&dir)?;

        let mut results = Vec::new();
        for info in &devices {
            warn_if_auth_required(info, password);
            let device = api::create_device(info.clone(), http_client.clone(), password.clone());
            let name_slug = slug_name(info.display_name());

            match device.config_get().await {
                Ok(config) => {
                    let backup = serde_json::json!({
                        "device": info.display_name(),
                        "ip": info.ip.to_string(),
                        "model": info.model,
                        "generation": info.generation.to_string(),
                        "mac": info.mac,
                        "firmware": info.firmware_version,
                        "backup_date": &today,
                        "config": config,
                    });
                    let filename = format!("{dir}/{name_slug}-{today}.json");
                    let data = serde_json::to_string_pretty(&backup)?;
                    std::fs::write(&filename, &data)?;

                    if json_output {
                        results.push(serde_json::json!({
                            "device": info.display_name(),
                            "file": filename,
                            "status": "ok",
                        }));
                    } else {
                        println!("{}: saved to {filename}", info.display_name());
                    }
                }
                Err(e) => {
                    if json_output {
                        results.push(serde_json::json!({
                            "device": info.display_name(),
                            "error": e.to_string(),
                        }));
                    } else {
                        eprintln!("{}: error: {e}", info.display_name());
                    }
                }
            }
        }

        if json_output {
            output::print_json_success(&results);
        }
    } else {
        let targets = resolve_and_probe_targets(cli, http_client, password).await?;
        let device = &targets[0];
        let info = device.info();
        let name_slug = slug_name(info.display_name());

        let config = device.config_get().await?;
        let backup = serde_json::json!({
            "device": info.display_name(),
            "ip": info.ip.to_string(),
            "model": info.model,
            "generation": info.generation.to_string(),
            "mac": info.mac,
            "firmware": info.firmware_version,
            "backup_date": &today,
            "config": config,
        });

        let dir = output_dir.unwrap_or_else(|| ".".to_string());
        std::fs::create_dir_all(&dir)?;
        let filename = format!("{dir}/{name_slug}-{today}.json");
        let data = serde_json::to_string_pretty(&backup)?;
        std::fs::write(&filename, &data)?;

        if json_output {
            output::print_json_success(&serde_json::json!({
                "device": info.display_name(),
                "file": filename,
            }));
        } else {
            println!("Backup saved to {filename}");
        }
    }

    Ok(())
}

async fn cmd_restore(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    file_path: &str,
    json_output: bool,
) -> Result<()> {
    let data = std::fs::read_to_string(file_path)
        .with_context(|| format!("failed to read backup file: {file_path}"))?;
    let backup: serde_json::Value =
        serde_json::from_str(&data).with_context(|| "invalid JSON in backup file")?;

    let config = backup
        .get("config")
        .ok_or_else(|| anyhow::anyhow!("backup file missing 'config' field"))?;

    let backup_device = backup
        .get("device")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let backup_gen = backup
        .get("generation")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let targets = resolve_and_probe_targets(cli, http_client, password).await?;
    let device = &targets[0];
    let info = device.info();

    if !json_output {
        eprintln!(
            "Restoring config from '{}' (backup of {}, {}) to {} ({})",
            file_path,
            backup_device,
            backup_gen,
            info.display_name(),
            info.generation,
        );
    }

    device.config_restore(config).await?;

    if json_output {
        output::print_json_success(&serde_json::json!({
            "device": info.display_name(),
            "backup_file": file_path,
            "backup_device": backup_device,
            "status": "restored",
        }));
    } else {
        println!(
            "{}: config restored. Device may need a reboot to apply all changes.",
            info.display_name()
        );
    }

    Ok(())
}

/// Convert a device name to a filesystem-safe slug.
fn generate_completions(shell: clap_complete::Shell) {
    use std::io::Write;

    // Generate the base completions
    let mut buf = Vec::new();
    clap_complete::generate(
        shell,
        &mut <Cli as clap::CommandFactory>::command(),
        "shelly",
        &mut buf,
    );
    let base = String::from_utf8(buf).unwrap();

    match shell {
        clap_complete::Shell::Zsh => {
            // Output base completions, then append device/group name completion
            print!("{base}");
            print!(
                r#"
# Dynamic device name completion for -n/--name
_shelly_device_names() {{
    local -a devices
    devices=(${{(f)"$(shelly _complete-device-names 2>/dev/null)"}})
    compadd -Q -- "${{devices[@]}}"
}}

# Dynamic group name completion for -g/--group
_shelly_group_names() {{
    local -a groups
    groups=(${{(f)"$(shelly _complete-group-names 2>/dev/null)"}})
    compadd -Q -- "${{groups[@]}}"
}}

# Hook into zsh completion system
zstyle ':completion:*:shelly:option-(-n|--name)-1:*' completer _shelly_device_names
zstyle ':completion:*:shelly:option-(-g|--group)-1:*' completer _shelly_group_names
"#
            );
        }
        clap_complete::Shell::Bash => {
            print!("{base}");
            print!(
                r#"
# Dynamic device name completion for -n/--name
_shelly_device_names() {{
    COMPREPLY=($(compgen -W "$(shelly _complete-device-names 2>/dev/null)" -- "${{COMP_WORDS[$COMP_CWORD]}}"))
}}

# Dynamic group name completion for -g/--group
_shelly_group_names() {{
    COMPREPLY=($(compgen -W "$(shelly _complete-group-names 2>/dev/null)" -- "${{COMP_WORDS[$COMP_CWORD]}}"))
}}

# Override completion for -n and -g flags
_shelly_dynamic() {{
    local prev="${{COMP_WORDS[COMP_CWORD-1]}}"
    case "$prev" in
        -n|--name)
            _shelly_device_names
            return
            ;;
        -g|--group)
            _shelly_group_names
            return
            ;;
    esac
    _shelly
}}
complete -F _shelly_dynamic -o default shelly
"#
            );
        }
        clap_complete::Shell::Fish => {
            print!("{base}");
            print!(
                r#"
# Dynamic device name completion for -n/--name
complete -c shelly -l name -s n -x -a "(shelly _complete-device-names 2>/dev/null)"

# Dynamic group name completion for -g/--group
complete -c shelly -l group -s g -x -a "(shelly _complete-group-names 2>/dev/null)"
"#
            );
        }
        _ => {
            // PowerShell and others: just output base completions
            print!("{base}");
        }
    }
    std::io::stdout().flush().unwrap();
}

fn slug_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

async fn cmd_reboot(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    json_output: bool,
) -> Result<()> {
    let targets = resolve_and_probe_targets(cli, http_client, password).await?;

    for device in &targets {
        device.reboot().await?;

        if json_output {
            output::print_json_success(&serde_json::json!({
                "device": device.info().display_name(),
                "status": "rebooting",
            }));
        } else {
            println!("Device {} is rebooting.", device.info().display_name());
        }
    }

    Ok(())
}

async fn cmd_rename(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    new_name: &str,
    json_output: bool,
) -> Result<()> {
    let targets = resolve_and_probe_targets(cli, http_client, password).await?;

    if targets.len() != 1 {
        anyhow::bail!(
            "rename requires exactly one target device (got {})",
            targets.len()
        );
    }

    let device = &targets[0];
    let old_name = device.info().display_name().to_string();
    device.set_name(new_name).await?;

    // Update the cached device list with the new name
    if let Ok(mut devices) = cache::load_devices()
        && let Some(cached) = devices.iter_mut().find(|d| d.ip == device.info().ip)
    {
        cached.name = Some(new_name.to_string());
        let _ = cache::save_devices(&devices);
    }

    if json_output {
        output::print_json_success(&serde_json::json!({
            "device": old_name,
            "new_name": new_name,
        }));
    } else {
        println!("Renamed '{}' → '{}'", old_name, new_name);
    }

    Ok(())
}

async fn cmd_watch(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    interval_secs: u64,
) -> Result<()> {
    let devices = resolve_all_or_group(cli)?;
    let interval = Duration::from_secs(interval_secs);
    watch::run(&devices, http_client, password.clone(), interval).await
}

async fn cmd_info(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    json_output: bool,
) -> Result<()> {
    let targets = resolve_and_probe_targets(cli, http_client, password).await?;
    let device = &targets[0];
    let info = device.info();
    let status = device.status().await?;

    if json_output {
        let json = output::device_info_json(info, &status);
        output::print_json_success(&json);
    } else {
        output::print_device_info(info, &status);
    }

    Ok(())
}

async fn cmd_health(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    json_output: bool,
) -> Result<()> {
    let devices = resolve_all_or_group(cli)?;

    let handles: Vec<_> = devices
        .iter()
        .map(|info| {
            let info = info.clone();
            let client = http_client.clone();
            let password = password.clone();
            tokio::spawn(async move { health::check_device(&info, &client, &password).await })
        })
        .collect();

    let mut reports = Vec::with_capacity(handles.len());
    for handle in handles {
        reports.push(handle.await?);
    }

    if json_output {
        output::print_json_success(&reports);
    } else {
        health::print_health_report(&reports);
    }

    Ok(())
}

fn cmd_group(action: GroupAction, json_output: bool) -> Result<()> {
    match action {
        GroupAction::List => groups::list_groups(json_output),
        GroupAction::Add { name, devices } => {
            groups::add_group(&name, devices.clone())?;
            if json_output {
                output::print_json_success(&serde_json::json!({
                    "group": name,
                    "devices": devices,
                }));
            } else {
                println!("Group '{name}' created with {} device(s).", devices.len());
            }
            Ok(())
        }
        GroupAction::Remove { name } => {
            groups::remove_group(&name)?;
            if json_output {
                output::print_json_success(&serde_json::json!({
                    "group": name,
                    "removed": true,
                }));
            } else {
                println!("Group '{name}' removed.");
            }
            Ok(())
        }
        GroupAction::Show { name } => groups::show_group(&name, json_output),
    }
}
