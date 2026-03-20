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
    ];

    let global_args: Vec<Value> = cmd
        .get_arguments()
        .filter(|a| {
            let id = a.get_id().as_str();
            id != "help" && id != "version"
        })
        .map(|a| {
            json!({
                "name": format!("--{}", a.get_id()),
                "required": a.is_required_set(),
                "description": a.get_help().map(|h| h.to_string()).unwrap_or_default(),
            })
        })
        .collect();

    let mut commands = serde_json::Map::new();

    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" {
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
        "global_flags": global_args,
        "commands": commands,
    })
}

fn build_args(cmd: &clap::Command) -> Vec<Value> {
    cmd.get_arguments()
        .filter(|a| {
            let id = a.get_id().as_str();
            id != "help" && id != "version"
        })
        .map(|a| {
            json!({
                "name": format!("--{}", a.get_id()),
                "required": a.is_required_set(),
                "description": a.get_help().map(|h| h.to_string()).unwrap_or_default(),
            })
        })
        .collect()
}
