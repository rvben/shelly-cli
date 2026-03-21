use clap::CommandFactory;
use serde_json::{Value, json};

use crate::cli::Cli;

/// Generate a machine-readable schema from clap's command introspection.
///
/// Nested subcommands (e.g. `switch on`, `firmware check`) are flattened into
/// keys like `"switch on"`, `"firmware check"`, etc.
pub fn generate_schema() -> Value {
    let cmd = Cli::command();
    let version = cmd.get_version().unwrap_or("unknown");

    let mutating_commands = [
        "on",
        "off",
        "toggle",
        "reboot",
        "switch on",
        "switch off",
        "switch toggle",
        "rename",
        "config set",
        "firmware update",
        "restore",
        "backup",
        "group add",
        "group remove",
    ];

    let global_args: Vec<Value> = cmd
        .get_arguments()
        .filter(|a| {
            let id = a.get_id().as_str();
            id != "help" && id != "version"
        })
        .map(build_arg_info)
        .collect();

    let mut commands = serde_json::Map::new();

    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" || name.starts_with('_') {
            continue;
        }

        let nested: Vec<_> = sub.get_subcommands().collect();

        if nested.iter().any(|s| s.get_name() != "help") {
            // Flatten nested subcommands: "switch on", "firmware check", etc.
            for nested_sub in &nested {
                if nested_sub.get_name() == "help" {
                    continue;
                }
                let flat_name = format!("{} {}", name, nested_sub.get_name());
                let args = build_args(nested_sub);
                let is_mutating = mutating_commands.contains(&flat_name.as_str());

                commands.insert(
                    flat_name,
                    json!({
                        "description": nested_sub.get_about().map(|h| h.to_string()).unwrap_or_default(),
                        "mutating": is_mutating,
                        "args": args,
                    }),
                );
            }
        } else {
            let args = build_args(sub);
            let is_mutating = mutating_commands.contains(&name);

            commands.insert(
                name.to_string(),
                json!({
                    "description": sub.get_about().map(|h| h.to_string()).unwrap_or_default(),
                    "mutating": is_mutating,
                    "args": args,
                }),
            );
        }
    }

    json!({
        "version": version,
        "binary": "shelly",
        "output_format": {
            "json": "Auto-enabled when piped. Use --json to force. Envelope: {\"ok\": true, \"data\": ...} or {\"ok\": false, \"error\": {\"code\": \"...\", \"message\": \"...\"}}",
            "error_codes": ["INVALID_INPUT", "DEVICE_NOT_FOUND", "DEVICE_UNREACHABLE", "NETWORK_ERROR", "AUTH_REQUIRED", "GROUP_NOT_FOUND", "NO_CACHED_DEVICES", "PARTIAL_FAILURE"],
        },
        "targeting": {
            "description": "Most commands require a target device. Use global flags or positional args to specify.",
            "flags": {
                "-n / --name": "Target by device name (from cache)",
                "--host": "Target by IP address",
                "-g / --group": "Target a device group",
                "-a / --all": "Target all cached devices (per-command flag)",
            },
            "positional": "on/off/toggle accept a device name as first positional arg: shelly on \"Kitchen Light\"",
        },
        "capabilities": {
            "description": "Device capabilities vary by generation. Use 'shelly devices' to see generation and num_outputs per device.",
            "Gen1": ["switch", "power", "energy", "config", "firmware", "backup", "webhooks"],
            "Gen2": ["switch", "power", "energy", "config", "firmware", "backup", "schedules", "webhooks"],
            "Gen3": ["switch", "power", "energy", "config", "firmware", "backup", "schedules", "webhooks"],
        },
        "global_flags": global_args,
        "commands": commands,
    })
}

fn build_args(cmd: &clap::Command) -> Vec<Value> {
    let positionals: Vec<Value> = cmd.get_positionals().map(build_positional_info).collect();

    let flags: Vec<Value> = cmd
        .get_arguments()
        .filter(|a| {
            let id = a.get_id().as_str();
            id != "help" && id != "version" && !a.is_positional()
        })
        .map(build_arg_info)
        .collect();

    let mut args = positionals;
    args.extend(flags);
    args
}

fn build_positional_info(a: &clap::Arg) -> Value {
    let id = a.get_id().as_str();

    let value_type = match id {
        "id" => "integer",
        _ => "string",
    };

    let mut info = json!({
        "name": id,
        "positional": true,
        "required": a.is_required_set(),
        "type": value_type,
        "description": a.get_help().map(|h| h.to_string()).unwrap_or_default(),
    });

    if let Some(default) = a.get_default_values().first() {
        info["default"] = json!(default.to_string_lossy());
    }

    info
}

fn build_arg_info(a: &clap::Arg) -> Value {
    let id = a.get_id().as_str();
    let takes_value = a.get_action().takes_values();

    let value_type = if !takes_value {
        "boolean"
    } else {
        match id {
            "id" | "timeout" | "interval" => "integer",
            _ => "string",
        }
    };

    let mut info = json!({
        "name": format!("--{}", id),
        "required": a.is_required_set(),
        "type": value_type,
        "description": a.get_help().map(|h| h.to_string()).unwrap_or_default(),
    });

    if let Some(default) = a.get_default_values().first() {
        info["default"] = json!(default.to_string_lossy());
    }

    info
}
