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
use std::net::IpAddr;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use owo_colors::OwoColorize;

use cli::{Cli, Command, ConfigAction, FirmwareAction, GroupAction, SwitchAction};
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
    let mut cli = Cli::parse();

    let json_output = cli.json || !std::io::stdout().is_terminal();
    let timeout = Duration::from_millis(cli.timeout);

    let http_client = reqwest::Client::builder()
        .timeout(timeout)
        .build()?;

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
        return cmd_switch(&cli, &http_client, action, json_output).await;
    }

    match cli.command {
        Command::Discover { subnet } => {
            cmd_discover(subnet, timeout, json_output, cli.quiet).await
        }
        Command::Devices { refresh } => {
            cmd_devices(refresh, timeout, json_output, cli.quiet).await
        }
        Command::Status { all } => {
            cmd_status(&cli, &http_client, all, json_output).await
        }
        Command::Switch { ref action } => {
            cmd_switch(&cli, &http_client, action.clone(), json_output).await
        }
        Command::Power { all, id } => {
            cmd_power(&cli, &http_client, all, id, json_output).await
        }
        Command::Firmware { ref action } => {
            cmd_firmware(&cli, &http_client, action.clone(), json_output).await
        }
        Command::Config { ref action } => {
            cmd_config(&cli, &http_client, action.clone()).await
        }
        Command::Reboot => {
            cmd_reboot(&cli, &http_client, json_output).await
        }
        Command::Watch { interval } => {
            cmd_watch(&cli, &http_client, interval).await
        }
        Command::Health => {
            cmd_health(&cli, &http_client, json_output).await
        }
        Command::Group { ref action } => {
            cmd_group(action.clone(), json_output)
        }
        Command::Schema => {
            let schema = schema::generate_schema();
            println!("{}", serde_json::to_string_pretty(&schema)?);
            Ok(())
        }
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut <Cli as clap::CommandFactory>::command(),
                "shelly",
                &mut std::io::stdout(),
            );
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
) -> Result<Vec<api::ShellyDevice>> {
    let infos = resolve_targets(cli)?;
    let mut devices = Vec::with_capacity(infos.len());

    for info in infos {
        let info = if info.id.is_empty() {
            api::probe_device(info.ip, http_client).await?
        } else {
            info
        };
        devices.push(api::create_device(info, http_client.clone()));
    }

    Ok(devices)
}

/// Load all cached devices, or resolve --group if specified.
fn resolve_all_or_group(cli: &Cli) -> Result<Vec<DeviceInfo>> {
    if let Some(ref group_name) = cli.group {
        return groups::resolve_group(group_name);
    }
    let devices = cache::load_devices()?;
    if devices.is_empty() {
        anyhow::bail!("no cached devices. Run 'shelly discover' first.");
    }
    Ok(devices)
}

fn colored_on_off(on: bool, color: bool) -> String {
    let color = color && output::use_color();
    if on {
        if color { "ON".green().to_string() } else { "ON".to_string() }
    } else if color {
        "OFF".dimmed().to_string()
    } else {
        "OFF".to_string()
    }
}

async fn cmd_discover(
    subnet_arg: Option<String>,
    timeout: Duration,
    json_output: bool,
    quiet: bool,
) -> Result<()> {
    let app_config = config::load_config()?;
    let subnet_str = subnet_arg
        .as_deref()
        .unwrap_or(&app_config.network.subnet);

    let subnet: ipnet::Ipv4Net = subnet_str
        .parse()
        .with_context(|| format!("invalid subnet: {subnet_str}"))?;

    if !quiet {
        eprintln!("Scanning {subnet}...");
    }

    let mut devices = api::discovery::scan_subnet(subnet, timeout, |info| {
        if !quiet && !json_output {
            eprintln!("  Found: {} at {}", info.display_name(), info.ip);
        }
    })
    .await?;

    let enrich_client = reqwest::Client::builder()
        .timeout(timeout)
        .build()?;

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
    all: bool,
    json_output: bool,
) -> Result<()> {
    if all || cli.group.is_some() {
        let devices = resolve_all_or_group(cli)?;

        let mut results = Vec::new();
        for info in &devices {
            let device = api::create_device(info.clone(), http_client.clone());
            match device.status().await {
                Ok(status) => {
                    if json_output {
                        results.push(serde_json::json!({
                            "device": info.display_name(),
                            "ip": info.ip.to_string(),
                            "status": status,
                        }));
                    } else {
                        output::print_status(info.display_name(), &status);
                        println!();
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
                        eprintln!("{}: {e}", info.display_name());
                    }
                }
            }
        }

        if json_output {
            output::print_json_success(&results);
        }
    } else {
        let targets = resolve_and_probe_targets(cli, http_client).await?;
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
    action: SwitchAction,
    json_output: bool,
) -> Result<()> {
    let targets = resolve_and_probe_targets(cli, http_client).await?;

    let mut json_results: Vec<serde_json::Value> = Vec::new();

    for device in &targets {
        let name = device.info().display_name().to_string();

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
                    json_results.push(serde_json::json!({ "device": name, "was_on": result.was_on }));
                } else {
                    let on_label = colored_on_off(true, !json_output);
                    let was_label = colored_on_off(result.was_on, !json_output);
                    println!("{name}: Switch {id} {on_label} (was {was_label})");
                }
            }
            SwitchAction::Off { id } => {
                let result = device.switch_set(id, false).await?;
                if json_output {
                    json_results.push(serde_json::json!({ "device": name, "was_on": result.was_on }));
                } else {
                    let off_label = colored_on_off(false, !json_output);
                    let was_label = colored_on_off(result.was_on, !json_output);
                    println!("{name}: Switch {id} {off_label} (was {was_label})");
                }
            }
            SwitchAction::Toggle { id } => {
                let result = device.switch_toggle(id).await?;
                if json_output {
                    json_results.push(serde_json::json!({ "device": name, "was_on": result.was_on }));
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
            let device = api::create_device(info.clone(), http_client.clone());
            match device.power(0).await {
                Ok(reading) => {
                    if json_output {
                        results.push(serde_json::json!({
                            "device": info.display_name(),
                            "ip": info.ip.to_string(),
                            "power": reading,
                        }));
                    } else {
                        output::print_power_reading(info.display_name(), &reading);
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
        let targets = resolve_and_probe_targets(cli, http_client).await?;
        let device = &targets[0];
        let reading = device.power(id).await?;

        if json_output {
            output::print_json_success(&reading);
        } else {
            output::print_power_reading(device.info().display_name(), &reading);
        }
    }

    Ok(())
}

async fn cmd_firmware(
    cli: &Cli,
    http_client: &reqwest::Client,
    action: FirmwareAction,
    json_output: bool,
) -> Result<()> {
    match action {
        FirmwareAction::Check { all } => {
            if all || cli.group.is_some() {
                let devices = resolve_all_or_group(cli)?;

                if !json_output {
                    let header = format!(
                        "{:<30} {:<16} {:<10} {:<20} {:<20}",
                        "Device", "IP", "Current", "Stable", "Beta"
                    );
                    if output::use_color() {
                        println!("{}", header.bold());
                        println!("{}", "-".repeat(96).dimmed());
                    } else {
                        println!("{header}");
                        println!("{}", "-".repeat(96));
                    }
                }

                let mut results = Vec::new();
                for info in &devices {
                    let device = api::create_device(info.clone(), http_client.clone());
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
                                println!(
                                    "{:<30} {:<16} {:<10} {:<20} {:<20}",
                                    info.display_name(),
                                    info.ip,
                                    fw.current_version,
                                    fw.stable_version.as_deref().unwrap_or("-"),
                                    fw.beta_version.as_deref().unwrap_or("-"),
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
                                eprintln!("{:<30} error: {e}", info.display_name());
                            }
                        }
                    }
                }

                if json_output {
                    output::print_json_success(&results);
                }
            } else {
                let targets = resolve_and_probe_targets(cli, http_client).await?;
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
    }

    Ok(())
}

async fn cmd_config(
    cli: &Cli,
    http_client: &reqwest::Client,
    action: ConfigAction,
) -> Result<()> {
    match action {
        ConfigAction::Get => {
            let targets = resolve_and_probe_targets(cli, http_client).await?;
            let device = &targets[0];
            let config = device.config_get().await?;
            output::print_json_success(&config);
        }
    }

    Ok(())
}

async fn cmd_reboot(
    cli: &Cli,
    http_client: &reqwest::Client,
    json_output: bool,
) -> Result<()> {
    let targets = resolve_and_probe_targets(cli, http_client).await?;

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

async fn cmd_watch(
    cli: &Cli,
    http_client: &reqwest::Client,
    interval_secs: u64,
) -> Result<()> {
    let devices = resolve_all_or_group(cli)?;
    let interval = Duration::from_secs(interval_secs);
    watch::run(&devices, http_client, interval).await
}

async fn cmd_health(
    cli: &Cli,
    http_client: &reqwest::Client,
    json_output: bool,
) -> Result<()> {
    let devices = resolve_all_or_group(cli)?;

    let handles: Vec<_> = devices
        .iter()
        .map(|info| {
            let info = info.clone();
            let client = http_client.clone();
            tokio::spawn(async move { health::check_device(&info, &client).await })
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
    }
}
