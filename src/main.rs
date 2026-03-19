mod api;
mod cache;
mod cli;
mod config;
mod model;
mod output;
mod schema;

use std::io::IsTerminal;
use std::net::IpAddr;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use cli::{Cli, Command, ConfigAction, FirmwareAction, SwitchAction};
use model::DeviceInfo;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let json_output = cli.json || !std::io::stdout().is_terminal();
    let timeout = Duration::from_millis(cli.timeout);

    let http_client = reqwest::Client::builder()
        .timeout(timeout)
        .build()?;

    match cli.command {
        Command::Discover { subnet } => {
            cmd_discover(subnet, timeout, json_output, cli.quiet).await
        }
        Command::Devices { refresh } => {
            cmd_devices(refresh, timeout, json_output, cli.quiet).await
        }
        Command::Status { all, id } => {
            cmd_status(&cli, &http_client, all, id, json_output).await
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
        Command::Schema => {
            let schema = schema::generate_schema();
            println!("{}", serde_json::to_string_pretty(&schema)?);
            Ok(())
        }
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut <Cli as clap::CommandFactory>::command(),
                "shelly-cli",
                &mut std::io::stdout(),
            );
            Ok(())
        }
    }
}

fn resolve_device(cli: &Cli, http_client: &reqwest::Client) -> Result<(DeviceInfo, reqwest::Client)> {
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
        Ok((info, http_client.clone()))
    } else if let Some(ref name) = cli.name {
        let devices = cache::load_devices()?;
        let info = cache::find_device_by_name(&devices, name)
            .ok_or_else(|| anyhow::anyhow!("device '{name}' not found in cache. Run 'shelly-cli discover' first."))?;
        Ok((info, http_client.clone()))
    } else {
        anyhow::bail!("specify --host <IP> or --name <NAME> to target a device")
    }
}

async fn resolve_and_probe(cli: &Cli, http_client: &reqwest::Client) -> Result<Box<dyn api::ShellyDevice>> {
    let (info, client) = resolve_device(cli, http_client)?;

    let info = if info.id.is_empty() {
        api::probe_device(info.ip, &client).await?
    } else {
        info
    };

    Ok(api::create_device(info, client))
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
        println!("{}", serde_json::to_string_pretty(&devices)?);
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
        eprintln!("No cached devices. Run 'shelly-cli discover' first.");
        return Ok(());
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&devices)?);
    } else {
        output::print_device_table(&devices);
    }

    Ok(())
}

async fn cmd_status(
    cli: &Cli,
    http_client: &reqwest::Client,
    all: bool,
    _id: u8,
    json_output: bool,
) -> Result<()> {
    if all {
        let devices = cache::load_devices()?;
        if devices.is_empty() {
            anyhow::bail!("no cached devices. Run 'shelly-cli discover' first.");
        }

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
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
    } else {
        let device = resolve_and_probe(cli, http_client).await?;
        let status = device.status().await?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&status)?);
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
    let device = resolve_and_probe(cli, http_client).await?;

    match action {
        SwitchAction::Status { id } => {
            let status = device.switch_status(id).await?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                output::print_switch_status(&status);
            }
        }
        SwitchAction::On { id } => {
            let result = device.switch_set(id, true).await?;
            if json_output {
                println!("{}", serde_json::json!({ "was_on": result.was_on }));
            } else {
                println!(
                    "Switch {id}: ON (was {})",
                    if result.was_on { "on" } else { "off" }
                );
            }
        }
        SwitchAction::Off { id } => {
            let result = device.switch_set(id, false).await?;
            if json_output {
                println!("{}", serde_json::json!({ "was_on": result.was_on }));
            } else {
                println!(
                    "Switch {id}: OFF (was {})",
                    if result.was_on { "on" } else { "off" }
                );
            }
        }
        SwitchAction::Toggle { id } => {
            let result = device.switch_toggle(id).await?;
            if json_output {
                println!("{}", serde_json::json!({ "was_on": result.was_on }));
            } else {
                println!(
                    "Switch {id}: TOGGLED (was {})",
                    if result.was_on { "on" } else { "off" }
                );
            }
        }
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
    if all {
        let devices = cache::load_devices()?;
        if devices.is_empty() {
            anyhow::bail!("no cached devices. Run 'shelly-cli discover' first.");
        }

        if !json_output {
            println!(
                "{:<30} {:>8} {:>7} {:>8} {:>12}",
                "Device", "Power", "Volt", "Current", "Total"
            );
            println!("{}", "-".repeat(70));
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
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
    } else {
        let device = resolve_and_probe(cli, http_client).await?;
        let reading = device.power(id).await?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&reading)?);
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
            if all {
                let devices = cache::load_devices()?;
                if devices.is_empty() {
                    anyhow::bail!("no cached devices. Run 'shelly-cli discover' first.");
                }

                if !json_output {
                    println!(
                        "{:<30} {:<16} {:<10} {:<20} {:<20}",
                        "Device", "IP", "Current", "Stable", "Beta"
                    );
                    println!("{}", "-".repeat(96));
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
                    println!("{}", serde_json::to_string_pretty(&results)?);
                }
            } else {
                let device = resolve_and_probe(cli, http_client).await?;
                let fw = device.firmware_check().await?;

                if json_output {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "current_version": fw.current_version,
                            "has_update": fw.has_update,
                            "stable_version": fw.stable_version,
                            "beta_version": fw.beta_version,
                        }))?
                    );
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
            let device = resolve_and_probe(cli, http_client).await?;
            let config = device.config_get().await?;
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
    }

    Ok(())
}

async fn cmd_reboot(
    cli: &Cli,
    http_client: &reqwest::Client,
    json_output: bool,
) -> Result<()> {
    let device = resolve_and_probe(cli, http_client).await?;
    device.reboot().await?;

    if json_output {
        println!("{}", serde_json::json!({ "status": "rebooting" }));
    } else {
        println!("Device {} is rebooting.", device.info().display_name());
    }

    Ok(())
}
