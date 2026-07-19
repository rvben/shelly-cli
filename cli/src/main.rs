mod cache;
mod cli;
mod color;
mod config;
mod errors;
mod groups;
mod health;
mod output;
mod schema;
mod watch;

use std::io::IsTerminal;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use futures::future::join_all;

use anyhow::{Context, Result};
use clap::{CommandFactory, FromArgMatches};
use owo_colors::OwoColorize;
use shelly_core::{api, model};

use cli::{
    Cli, Command, ConfigAction, FirmwareAction, GroupAction, LightAction, ListArgs, ScheduleAction,
    SwitchAction, WebhookAction,
};
use model::DeviceInfo;
use output::OutputFormat;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        let cli_error = errors::classify_error(&err);
        let exit_code = cli_error.exit_code;

        // Resolve output format from raw args since Cli may not have parsed yet.
        let json_mode = resolve_json_mode_from_args();

        if json_mode {
            output::print_json_error(&cli_error);
        } else {
            eprintln!("Error: {}", cli_error.message);
            if let Some(ref hint) = cli_error.hint {
                eprintln!("Hint: {hint}");
            }
        }

        std::process::exit(exit_code);
    }
}

/// Strip ANSI escape sequences from a string for plain-text error messages.
fn strip_ansi(s: &str) -> String {
    // Matches ESC[ ... m and similar CSI sequences.
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            // Consume ESC[
            chars.next();
            // Consume until a letter (the command character)
            for ch in chars.by_ref() {
                if ch.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Determine whether to emit JSON errors from raw argv, before clap runs.
fn resolve_json_mode_from_args() -> bool {
    let args: Vec<String> = std::env::args().collect();

    // --json or -j flags always mean JSON mode
    if args.iter().any(|a| a == "--json" || a == "-j") {
        return true;
    }

    // --output json or -o json means JSON mode
    for window in args.windows(2) {
        if (window[0] == "--output" || window[0] == "-o") && window[1] == "json" {
            return true;
        }
    }
    if args.iter().any(|a| a == "--output=json" || a == "-o=json") {
        return true;
    }

    // Default: auto-detect by TTY
    !std::io::stdout().is_terminal()
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

    // Use try_get_matches so clap parse errors are returned as anyhow::Error
    // rather than printed directly, letting our structured error handler emit JSON.
    let matches = Cli::command()
        .name(bin_name)
        .try_get_matches()
        .map_err(|e| {
            // Help and version requests are normal exits, not errors.
            // Let clap handle them as intended.
            if e.kind() == clap::error::ErrorKind::DisplayHelp
                || e.kind() == clap::error::ErrorKind::DisplayVersion
                || e.kind() == clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            {
                let _ = e.print();
                std::process::exit(0);
            }
            // Strip ANSI escape codes so the message is plain text.
            let rendered = e.render().to_string();
            let plain = strip_ansi(&rendered);
            // Embed a sentinel prefix so classify_error recognises clap errors.
            anyhow::anyhow!("clap_error: {plain}")
        })?;
    let mut cli = Cli::from_arg_matches(&matches)?;

    // Resolve three-valued format: --json (hidden alias) overrides --output; auto detects TTY.
    let format = if cli.json {
        OutputFormat::Json
    } else {
        match cli.output.as_str() {
            "json" => OutputFormat::Json,
            "text" => OutputFormat::Text,
            _ => OutputFormat::Auto,
        }
    };
    let json_output = format.is_json();
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
        Command::Devices { refresh, list } => {
            cmd_devices(refresh, timeout, json_output, cli.quiet, list).await
        }
        Command::Status { all, ref list } => {
            cmd_status(
                &cli,
                &http_client,
                &password,
                all,
                list.clone(),
                json_output,
            )
            .await
        }
        Command::Switch { ref action } => {
            cmd_switch(&cli, &http_client, &password, action.clone(), json_output).await
        }
        Command::Light { ref action } => {
            cmd_light(&cli, &http_client, &password, action.clone(), json_output).await
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
        Command::Backup { all, ref dir } => {
            cmd_backup(&cli, &http_client, &password, all, dir.clone(), json_output).await
        }
        Command::Restore { ref file, yes } => {
            cmd_restore(&cli, &http_client, &password, file, yes, json_output).await
        }
        Command::Rename { ref new_name, yes } => {
            cmd_rename(&cli, &http_client, &password, new_name, yes, json_output).await
        }
        Command::Reboot { yes } => {
            cmd_reboot(&cli, &http_client, &password, yes, json_output).await
        }
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

/// Apply limit/offset pagination and optional field filtering to a JSON array.
fn paginate(
    items: Vec<serde_json::Value>,
    limit: usize,
    offset: usize,
    fields: &Option<String>,
) -> Vec<serde_json::Value> {
    let field_list: Option<Vec<&str>> = fields
        .as_deref()
        .map(|f| f.split(',').map(str::trim).collect());

    items
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|item| {
            if let (Some(fl), Some(obj)) = (&field_list, item.as_object()) {
                let filtered: serde_json::Map<_, _> = obj
                    .iter()
                    .filter(|(k, _)| fl.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                serde_json::Value::Object(filtered)
            } else {
                item
            }
        })
        .collect()
}

async fn cmd_devices(
    refresh: bool,
    timeout: Duration,
    json_output: bool,
    quiet: bool,
    list: ListArgs,
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
        let all_items: Vec<serde_json::Value> = devices
            .iter()
            .map(|d| serde_json::to_value(d).unwrap_or_default())
            .collect();
        let total = all_items.len();
        let page = paginate(all_items, list.limit, list.offset, &list.fields);
        output::print_json_success(&serde_json::json!({
            "items": page,
            "total": total,
            "limit": list.limit,
            "offset": list.offset,
        }));
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
    list: ListArgs,
    json_output: bool,
) -> Result<()> {
    if all || cli.group.is_some() {
        let devices = resolve_all_or_group(cli)?;

        for info in &devices {
            warn_if_auth_required(info, password);
        }

        // Query all devices in parallel
        let futures: Vec<_> = devices
            .iter()
            .map(|info| {
                let device =
                    api::create_device(info.clone(), http_client.clone(), password.clone());
                async move { device.status().await }
            })
            .collect();
        let statuses = join_all(futures).await;

        if json_output {
            let all_items: Vec<serde_json::Value> = devices
                .iter()
                .zip(statuses.iter())
                .map(|(info, result)| match result {
                    Ok(status) => serde_json::json!({
                        "device": info.display_name(),
                        "ip": info.ip.to_string(),
                        "status": status,
                    }),
                    Err(e) => serde_json::json!({
                        "device": info.display_name(),
                        "ip": info.ip.to_string(),
                        "error": e.to_string(),
                    }),
                })
                .collect();
            let total = all_items.len();
            let page = paginate(all_items, list.limit, list.offset, &list.fields);
            output::print_json_success(&serde_json::json!({
                "items": page,
                "total": total,
                "limit": list.limit,
                "offset": list.offset,
            }));
        } else {
            output::print_status_table_header();
            for (info, result) in devices.iter().zip(statuses.iter()) {
                match result {
                    Ok(status) => output::print_status_table_row(
                        info.display_name(),
                        &info.ip.to_string(),
                        status,
                    ),
                    Err(e) => output::print_status_table_error(
                        info.display_name(),
                        &info.ip.to_string(),
                        &e.to_string(),
                    ),
                }
            }
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

async fn cmd_light(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    action: LightAction,
    json_output: bool,
) -> Result<()> {
    let targets = resolve_and_probe_targets(cli, http_client, password).await?;
    let mut json_results: Vec<serde_json::Value> = Vec::new();

    for device in &targets {
        let name = device.info().display_name().to_string();
        let components = device.light_components().await?;

        let id = match &action {
            LightAction::Status { id } | LightAction::Off { id } | LightAction::Toggle { id } => {
                *id
            }
            LightAction::On { args } | LightAction::Set { args } => args.id,
        };
        let kind = validate_light_id(&components, id, &name)?;

        match &action {
            LightAction::Status { .. } => {
                let status = device.light_status(kind, id).await?;
                if json_output {
                    json_results.push(serde_json::json!({ "device": name, "status": status }));
                } else {
                    if targets.len() > 1 {
                        print!("{name}: ");
                    }
                    output::print_light_status(&status);
                }
            }
            LightAction::On { args } => {
                let mut params = build_light_params(
                    kind,
                    &name,
                    &args.color,
                    &args.rgb,
                    args.brightness,
                    args.white,
                    args.temp,
                )?;
                params.on = Some(true);
                let result = device.light_set(kind, id, &params).await?;
                if json_output {
                    json_results
                        .push(serde_json::json!({ "device": name, "was_on": result.was_on }));
                } else {
                    let on_label = colored_on_off(true, !json_output);
                    let was_label = colored_on_off(result.was_on, !json_output);
                    println!("{name}: Light {id} {on_label} (was {was_label})");
                }
            }
            LightAction::Off { .. } => {
                let params = model::LightParams {
                    on: Some(false),
                    ..Default::default()
                };
                let result = device.light_set(kind, id, &params).await?;
                if json_output {
                    json_results
                        .push(serde_json::json!({ "device": name, "was_on": result.was_on }));
                } else {
                    let off_label = colored_on_off(false, !json_output);
                    let was_label = colored_on_off(result.was_on, !json_output);
                    println!("{name}: Light {id} {off_label} (was {was_label})");
                }
            }
            LightAction::Toggle { .. } => {
                let result = device.light_toggle(kind, id).await?;
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
                    println!("{name}: Light {id} {toggled} (was {was_label})");
                }
            }
            LightAction::Set { args } => {
                let mut params = build_light_params(
                    kind,
                    &name,
                    &args.color,
                    &args.rgb,
                    args.brightness,
                    args.white,
                    args.temp,
                )?;
                // `on` is set below from the device's current state, so only the
                // attribute fields are checked here: `set` must change something.
                if params.rgb.is_none()
                    && params.brightness.is_none()
                    && params.white.is_none()
                    && params.ct.is_none()
                {
                    anyhow::bail!(
                        "light set requires at least one of --color/--rgb, --brightness, --white, --temp"
                    );
                }
                // Read and resend the current power state so `set` never toggles
                // the light. When only color is changed this `on` value is also
                // what satisfies the device's "at least one of on/brightness" rule.
                let current = device.light_status(kind, id).await?;
                params.on = Some(current.output);
                let _ = device.light_set(kind, id, &params).await?;
                if json_output {
                    json_results.push(serde_json::json!({ "device": name, "id": id }));
                } else {
                    println!("{name}: Light {id} updated");
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

        for info in &devices {
            warn_if_auth_required(info, password);
        }

        // Query all devices in parallel (meters within each device are sequential)
        let futures: Vec<_> = devices
            .iter()
            .map(|info| {
                let device =
                    api::create_device(info.clone(), http_client.clone(), password.clone());
                let num_meters = info.num_meters;
                async move {
                    let mut readings = Vec::new();
                    for meter_id in 0..num_meters {
                        readings.push((meter_id, device.power(meter_id).await));
                    }
                    readings
                }
            })
            .collect();
        let all_readings = join_all(futures).await;

        let mut results = Vec::new();
        for (info, readings) in devices.iter().zip(all_readings.iter()) {
            for (meter_id, result) in readings {
                let label = if info.num_meters > 1 {
                    format!("{} [{}]", info.display_name(), meter_id)
                } else {
                    info.display_name().to_string()
                };
                match result {
                    Ok(reading) => {
                        if json_output {
                            results.push(serde_json::json!({
                                "device": info.display_name(),
                                "ip": info.ip.to_string(),
                                "meter_id": meter_id,
                                "power": reading,
                            }));
                        } else {
                            output::print_power_reading(&label, reading);
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

        for info in &devices {
            warn_if_auth_required(info, password);
        }

        // Query all devices in parallel
        let futures: Vec<_> = devices
            .iter()
            .map(|info| {
                let device =
                    api::create_device(info.clone(), http_client.clone(), password.clone());
                let num_meters = info.num_meters;
                async move {
                    let mut total_wh = 0.0;
                    let mut error: Option<String> = None;
                    for meter_id in 0..num_meters {
                        match device.power(meter_id).await {
                            Ok(reading) => total_wh += reading.total_energy_wh,
                            Err(e) => {
                                error = Some(e.to_string());
                                break;
                            }
                        }
                    }
                    (num_meters, total_wh, error)
                }
            })
            .collect();
        let all_energy = join_all(futures).await;

        let mut results = Vec::new();
        let mut grand_total_kwh = 0.0;

        for (info, (num_meters, total_wh, error)) in devices.iter().zip(all_energy) {
            let name = info.display_name().to_string();

            if num_meters == 0 {
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

            if let Some(e) = error {
                if json_output {
                    results.push(serde_json::json!({
                        "device": name,
                        "ip": info.ip.to_string(),
                        "error": e,
                    }));
                } else {
                    eprintln!("{:<34} error: {e}", name);
                }
            } else {
                let kwh = total_wh / 1000.0;
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

/// Validate that a light component ID exists on the device and return its kind.
///
/// Assumes a device exposes at most one light-class component kind, which holds
/// for current Shelly hardware (a device is RGB, or RGBW, or CCT, or a dimmer,
/// never a mix). The first component matching `id` wins.
fn validate_light_id(
    components: &[model::LightComponent],
    id: u8,
    device_name: &str,
) -> Result<model::LightKind> {
    if let Some(c) = components.iter().find(|c| c.id == id) {
        return Ok(c.kind);
    }
    if components.is_empty() {
        anyhow::bail!(
            "{device_name} has no RGB/light outputs. 'light' supports Gen2/Gen3 RGB, RGBW, CCT, and dimmable devices."
        );
    }
    let ids: Vec<String> = components.iter().map(|c| c.id.to_string()).collect();
    anyhow::bail!(
        "light ID {id} is out of range for {device_name} (valid IDs: {})",
        ids.join(", ")
    );
}

/// Validate flags against the component kind and build params (color/brightness/
/// white/temp). The caller sets `on` separately.
fn build_light_params(
    kind: model::LightKind,
    device_name: &str,
    color: &Option<String>,
    rgb: &Option<String>,
    brightness: Option<u8>,
    white: Option<u8>,
    temp: Option<u32>,
) -> Result<model::LightParams> {
    let mut params = model::LightParams::default();

    if (color.is_some() || rgb.is_some()) && !kind.supports_rgb() {
        anyhow::bail!(
            "color is not supported on {device_name}'s {} output; use --brightness{}",
            kind.as_str(),
            if kind.supports_ct() { " or --temp" } else { "" }
        );
    }
    if white.is_some() && !kind.supports_white() {
        anyhow::bail!(
            "--white is only valid for RGBW lights; {device_name} has a {} output",
            kind.as_str()
        );
    }
    if temp.is_some() && !kind.supports_ct() {
        anyhow::bail!(
            "--temp is only valid for color-temperature (cct) lights; {device_name} has a {} output",
            kind.as_str()
        );
    }

    if let Some(c) = color {
        params.rgb = Some(color::parse_color(c)?.to_array());
    } else if let Some(t) = rgb {
        params.rgb = Some(color::parse_rgb_triple(t)?.to_array());
    }

    if let Some(b) = brightness {
        let min = kind.brightness_min();
        if b < min || b > 100 {
            anyhow::bail!(
                "--brightness for {} lights must be {min}-100, got {b}",
                kind.as_str()
            );
        }
        params.brightness = Some(b);
    }

    params.white = white;
    params.ct = temp;
    Ok(params)
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
        FirmwareAction::Update { all, yes } => {
            errors::check_confirmation(yes, "firmware update")?;
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
        ScheduleAction::List { all, list } => {
            if all || cli.group.is_some() {
                let devices = resolve_all_or_group(cli)?;
                let mut all_items: Vec<serde_json::Value> = Vec::new();
                for info in &devices {
                    warn_if_auth_required(info, password);
                    let device =
                        api::create_device(info.clone(), http_client.clone(), password.clone());
                    match device.schedule_list().await {
                        Ok(schedules) => {
                            if let Some(scheds) = schedules.as_array() {
                                for s in scheds {
                                    let mut entry = serde_json::json!({
                                        "device": info.display_name(),
                                        "ip": info.ip.to_string(),
                                    });
                                    if let Some(obj) = s.as_object() {
                                        for (k, v) in obj {
                                            entry[k] = v.clone();
                                        }
                                    }
                                    all_items.push(entry);
                                }
                            }
                        }
                        Err(e) => {
                            if !json_output {
                                eprintln!("{}: {e}", info.display_name());
                            }
                        }
                    }
                }
                if json_output {
                    let total = all_items.len();
                    let page = paginate(all_items, list.limit, list.offset, &list.fields);
                    output::print_json_success(&serde_json::json!({
                        "items": page,
                        "total": total,
                        "limit": list.limit,
                        "offset": list.offset,
                    }));
                } else {
                    for item in &all_items {
                        let name = item["device"].as_str().unwrap_or("?");
                        let id = item.get("id").and_then(|v| v.as_i64()).unwrap_or(-1);
                        let enabled = item
                            .get("enable")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let timespec = item.get("timespec").and_then(|v| v.as_str()).unwrap_or("?");
                        let status = if enabled { "enabled" } else { "disabled" };
                        println!("{name}: [{id}] {timespec} ({status})");
                    }
                }
            } else {
                let targets = resolve_and_probe_targets(cli, http_client, password).await?;
                let device = &targets[0];
                let schedules = device.schedule_list().await?;
                if json_output {
                    let all_items: Vec<serde_json::Value> =
                        schedules.as_array().cloned().unwrap_or_default();
                    let total = all_items.len();
                    let page = paginate(all_items, list.limit, list.offset, &list.fields);
                    output::print_json_success(&serde_json::json!({
                        "items": page,
                        "total": total,
                        "limit": list.limit,
                        "offset": list.offset,
                    }));
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
        WebhookAction::List { all, list } => {
            if all || cli.group.is_some() {
                let devices = resolve_all_or_group(cli)?;
                let mut all_items: Vec<serde_json::Value> = Vec::new();
                for info in &devices {
                    warn_if_auth_required(info, password);
                    let device =
                        api::create_device(info.clone(), http_client.clone(), password.clone());
                    match device.webhook_list().await {
                        Ok(hooks) => {
                            if let Some(hook_arr) = hooks.as_array() {
                                for h in hook_arr {
                                    let mut entry = serde_json::json!({
                                        "device": info.display_name(),
                                        "ip": info.ip.to_string(),
                                    });
                                    if let Some(obj) = h.as_object() {
                                        for (k, v) in obj {
                                            entry[k] = v.clone();
                                        }
                                    }
                                    all_items.push(entry);
                                }
                            }
                        }
                        Err(e) => {
                            if !json_output {
                                eprintln!("{}: {e}", info.display_name());
                            }
                        }
                    }
                }
                if json_output {
                    let total = all_items.len();
                    let page = paginate(all_items, list.limit, list.offset, &list.fields);
                    output::print_json_success(&serde_json::json!({
                        "items": page,
                        "total": total,
                        "limit": list.limit,
                        "offset": list.offset,
                    }));
                } else {
                    for item in &all_items {
                        print_webhook_entry(item);
                    }
                }
            } else {
                let targets = resolve_and_probe_targets(cli, http_client, password).await?;
                let device = &targets[0];
                let hooks = device.webhook_list().await?;
                if json_output {
                    let all_items: Vec<serde_json::Value> =
                        hooks.as_array().cloned().unwrap_or_default();
                    let total = all_items.len();
                    let page = paginate(all_items, list.limit, list.offset, &list.fields);
                    output::print_json_success(&serde_json::json!({
                        "items": page,
                        "total": total,
                        "limit": list.limit,
                        "offset": list.offset,
                    }));
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
    yes: bool,
    json_output: bool,
) -> Result<()> {
    errors::check_confirmation(yes, "restore device configuration")?;
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
    yes: bool,
    json_output: bool,
) -> Result<()> {
    errors::check_confirmation(yes, "reboot device(s)")?;
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
    yes: bool,
    json_output: bool,
) -> Result<()> {
    errors::check_confirmation(yes, "rename device")?;
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
        println!("Renamed '{}' to '{}'", old_name, new_name);
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
        GroupAction::List { list } => {
            groups::list_groups(json_output, list.limit, list.offset, &list.fields)
        }
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
        GroupAction::Remove { name, yes } => {
            errors::check_confirmation(yes, &format!("remove group '{name}'"))?;
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

#[cfg(test)]
mod light_tests {
    use super::*;
    use model::{LightComponent, LightKind};

    #[test]
    fn validate_light_id_returns_kind() {
        let comps = vec![LightComponent {
            kind: LightKind::Rgb,
            id: 0,
        }];
        assert_eq!(
            validate_light_id(&comps, 0, "Lamp").unwrap(),
            LightKind::Rgb
        );
    }

    #[test]
    fn validate_light_id_no_components_errors() {
        let err = validate_light_id(&[], 0, "Switch1")
            .unwrap_err()
            .to_string();
        assert!(err.contains("no RGB/light outputs"));
    }

    #[test]
    fn validate_light_id_out_of_range_errors() {
        let comps = vec![LightComponent {
            kind: LightKind::Rgb,
            id: 0,
        }];
        let err = validate_light_id(&comps, 2, "Lamp")
            .unwrap_err()
            .to_string();
        assert!(err.contains("out of range"));
        assert!(err.contains("valid IDs: 0"));
    }

    #[test]
    fn build_params_rejects_color_on_cct() {
        let err = build_light_params(
            LightKind::Cct,
            "Bulb",
            &Some("red".to_string()),
            &None,
            None,
            None,
            None,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("color is not supported"));
        assert!(err.contains("--temp"));
    }

    #[test]
    fn build_params_rejects_white_on_rgb() {
        let err = build_light_params(LightKind::Rgb, "Lamp", &None, &None, None, Some(100), None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("--white is only valid for RGBW"));
    }

    #[test]
    fn build_params_rejects_temp_on_rgb() {
        let err = build_light_params(LightKind::Rgb, "Lamp", &None, &None, None, None, Some(3000))
            .unwrap_err()
            .to_string();
        assert!(err.contains("--temp is only valid"));
    }

    #[test]
    fn build_params_rejects_brightness_zero_on_rgb() {
        let err = build_light_params(LightKind::Rgb, "Lamp", &None, &None, Some(0), None, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("must be 1-100"));
    }

    #[test]
    fn build_params_allows_brightness_zero_on_cct() {
        let params =
            build_light_params(LightKind::Cct, "Bulb", &None, &None, Some(0), None, None).unwrap();
        assert_eq!(params.brightness, Some(0));
    }

    #[test]
    fn build_params_rejects_brightness_over_100() {
        let err = build_light_params(LightKind::Cct, "Bulb", &None, &None, Some(101), None, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("must be 0-100"));
    }

    #[test]
    fn build_params_builds_rgb_and_brightness() {
        let params = build_light_params(
            LightKind::Rgb,
            "Lamp",
            &Some("#00ff88".to_string()),
            &None,
            Some(80),
            None,
            None,
        )
        .unwrap();
        assert_eq!(params.rgb, Some([0, 255, 136]));
        assert_eq!(params.brightness, Some(80));
        assert_eq!(params.white, None);
        assert_eq!(params.ct, None);
    }
}
