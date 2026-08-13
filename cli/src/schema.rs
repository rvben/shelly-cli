use std::collections::HashMap;

use clap::CommandFactory;
use serde_json::{Value, json};

use crate::cli::Cli;

/// Static metadata that cannot be derived from clap alone.
struct CommandMeta {
    mutating: bool,
    output_fields: &'static [(&'static str, &'static str)],
}

fn build_metadata() -> HashMap<&'static str, CommandMeta> {
    macro_rules! meta {
        ($path:expr, mutating: $mut:expr, fields: [$($name:expr => $typ:expr),* $(,)?]) => {
            ($path, CommandMeta {
                mutating: $mut,
                output_fields: &[$(($name, $typ)),*],
            })
        };
    }

    HashMap::from([
        meta!("discover", mutating: false, fields: [
            "name" => "string", "ip" => "string", "model" => "string",
            "generation" => "string", "mac" => "string", "firmware_version" => "string",
        ]),
        meta!("devices", mutating: false, fields: [
            "name" => "string", "ip" => "string", "model" => "string",
            "generation" => "string", "mac" => "string", "firmware_version" => "string",
        ]),
        meta!("status", mutating: false, fields: [
            "device" => "string", "ip" => "string",
            "uptime" => "integer | null", "temperature_c" => "number | null",
            "switches" => "array",
        ]),
        meta!("switch status", mutating: false, fields: [
            "device" => "string", "id" => "integer", "output" => "boolean",
            "power_watts" => "number | null",
        ]),
        meta!("switch on", mutating: true, fields: [
            "device" => "string", "was_on" => "boolean",
        ]),
        meta!("switch off", mutating: true, fields: [
            "device" => "string", "was_on" => "boolean",
        ]),
        meta!("switch toggle", mutating: true, fields: [
            "device" => "string", "was_on" => "boolean",
        ]),
        meta!("on", mutating: true, fields: [
            "device" => "string", "was_on" => "boolean",
        ]),
        meta!("off", mutating: true, fields: [
            "device" => "string", "was_on" => "boolean",
        ]),
        meta!("toggle", mutating: true, fields: [
            "device" => "string", "was_on" => "boolean",
        ]),
        meta!("light status", mutating: false, fields: [
            "device" => "string", "id" => "integer", "output" => "boolean",
            "brightness" => "number | null", "rgb" => "array | null",
        ]),
        meta!("light on", mutating: true, fields: [
            "device" => "string", "was_on" => "boolean",
        ]),
        meta!("light off", mutating: true, fields: [
            "device" => "string", "was_on" => "boolean",
        ]),
        meta!("light toggle", mutating: true, fields: [
            "device" => "string", "was_on" => "boolean",
        ]),
        meta!("light set", mutating: true, fields: [
            "device" => "string", "id" => "integer",
        ]),
        meta!("power", mutating: false, fields: [
            "device" => "string", "power_watts" => "number",
            "voltage" => "number | null", "current" => "number | null",
            "total_energy_wh" => "number",
        ]),
        meta!("energy", mutating: false, fields: [
            "device" => "string", "total_kwh" => "number",
        ]),
        meta!("firmware check", mutating: false, fields: [
            "device" => "string", "firmware" => "string", "has_update" => "boolean",
            "stable" => "string | null", "beta" => "string | null",
        ]),
        meta!("firmware update", mutating: true, fields: [
            "device" => "string", "status" => "string", "from" => "string | null",
        ]),
        meta!("config get", mutating: false, fields: []),
        meta!("config set", mutating: true, fields: [
            "device" => "string", "key" => "string", "value" => "string",
            "status" => "string",
        ]),
        meta!("schedule list", mutating: false, fields: [
            "device" => "string", "id" => "integer",
            "timespec" => "string", "enable" => "boolean",
        ]),
        meta!("webhook list", mutating: false, fields: [
            "device" => "string", "id" => "integer",
            "name" => "string", "event" => "string", "enable" => "boolean",
        ]),
        meta!("backup", mutating: false, fields: [
            "device" => "string", "file" => "string",
        ]),
        meta!("restore", mutating: true, fields: [
            "device" => "string", "backup_file" => "string", "status" => "string",
        ]),
        meta!("rename", mutating: true, fields: [
            "device" => "string", "new_name" => "string",
        ]),
        meta!("reboot", mutating: true, fields: [
            "device" => "string", "status" => "string",
        ]),
        meta!("info", mutating: false, fields: [
            "name" => "string", "model" => "string", "generation" => "string",
            "ip" => "string", "mac" => "string", "firmware" => "string",
            "uptime_seconds" => "integer | null", "switches" => "array",
        ]),
        meta!("health", mutating: false, fields: [
            "device" => "string", "online" => "boolean", "status" => "string",
        ]),
        meta!("group list", mutating: false, fields: [
            "name" => "string", "devices" => "array",
        ]),
        meta!("group add", mutating: true, fields: [
            "group" => "string", "devices" => "array",
        ]),
        meta!("group remove", mutating: true, fields: [
            "group" => "string", "removed" => "boolean",
        ]),
        meta!("group show", mutating: false, fields: [
            "name" => "string", "ip" => "string", "model" => "string",
            "generation" => "string",
        ]),
        meta!("schema", mutating: false, fields: []),
        meta!("capabilities", mutating: false, fields: [
            "generations" => "array", "features" => "array", "structured_output" => "boolean",
        ]),
        meta!("completions", mutating: false, fields: []),
    ])
}

/// The set of global arg IDs that appear on every command and should not be
/// repeated inside per-command `args` arrays.
const GLOBAL_ARG_IDS: &[&str] = &[
    "help", "version", "host", "name", "group", "output", "json", "quiet", "password", "timeout",
];

fn arg_to_json(a: &clap::Arg) -> Value {
    let id = a.get_id().as_str();
    let takes_value = a.get_action().takes_values();

    let value_type = if !takes_value {
        "boolean"
    } else {
        match id {
            "id" | "timeout" | "interval" | "limit" | "offset" => "integer",
            _ => "string",
        }
    };

    let flag_name = if a.is_positional() {
        id.to_string()
    } else {
        format!("--{id}")
    };

    let mut info = json!({
        "name": flag_name,
        "type": value_type,
        "required": a.is_required_set(),
        "description": a.get_help().map(|h| h.to_string()).unwrap_or_default(),
    });

    if a.is_positional() {
        info["positional"] = json!(true);
    }

    if let Some(default) = a.get_default_values().first() {
        info["default"] = json!(default.to_string_lossy());
    }

    if let Some(possible) = {
        let vals: Vec<_> = a.get_possible_values().into_iter().collect();
        if vals.is_empty() { None } else { Some(vals) }
    } {
        let enum_vals: Vec<Value> = possible.iter().map(|v| json!(v.get_name())).collect();
        info["enum"] = json!(enum_vals);
    }

    info
}

fn walk_commands(
    cmd: &clap::Command,
    prefix: &str,
    metadata: &HashMap<&str, CommandMeta>,
    out: &mut Vec<Value>,
) {
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" || name.starts_with('_') || sub.is_hide_set() {
            continue;
        }

        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix} {name}")
        };

        let real_subcommands: Vec<_> = sub
            .get_subcommands()
            .filter(|s| s.get_name() != "help")
            .collect();

        if !real_subcommands.is_empty() {
            walk_commands(sub, &path, metadata, out);
            continue;
        }

        // Collect args that are not global-level flags
        let args: Vec<Value> = sub
            .get_arguments()
            .filter(|a| {
                let id = a.get_id().as_str();
                !GLOBAL_ARG_IDS.contains(&id)
            })
            .map(arg_to_json)
            .collect();

        let meta = metadata.get(path.as_str());

        let mut entry = serde_json::Map::new();
        entry.insert("name".into(), json!(path));
        if let Some(about) = sub.get_about().map(|a| a.to_string())
            && !about.is_empty()
        {
            entry.insert("description".into(), json!(about));
        }
        entry.insert("mutating".into(), json!(meta.is_some_and(|m| m.mutating)));
        entry.insert("args".into(), json!(args));

        if let Some(m) = meta
            && !m.output_fields.is_empty()
        {
            let fields: Vec<Value> = m
                .output_fields
                .iter()
                .map(|(n, t)| json!({"name": n, "type": t}))
                .collect();
            entry.insert("output_fields".into(), json!(fields));
        }

        out.push(Value::Object(entry));
    }
}

fn errors_schema() -> Value {
    json!([
        {
            "kind": "invalid_input",
            "exit_code": 1,
            "retryable": false,
            "description": "Invalid argument or unsupported parameter value"
        },
        {
            "kind": "device_not_found",
            "exit_code": 1,
            "retryable": false,
            "description": "Device name not found in the local cache"
        },
        {
            "kind": "no_cached_devices",
            "exit_code": 1,
            "retryable": false,
            "description": "No devices discovered yet; run 'shelly discover' first"
        },
        {
            "kind": "group_not_found",
            "exit_code": 1,
            "retryable": false,
            "description": "Named group not found in groups.toml"
        },
        {
            "kind": "device_unreachable",
            "exit_code": 2,
            "retryable": true,
            "description": "Device did not respond within the configured timeout"
        },
        {
            "kind": "confirmation_required",
            "exit_code": 2,
            "retryable": false,
            "description": "Destructive command requires explicit --yes when stdin is not a terminal"
        },
        {
            "kind": "network_error",
            "exit_code": 2,
            "retryable": true,
            "description": "Network or HTTP error communicating with the device"
        },
        {
            "kind": "auth_required",
            "exit_code": 3,
            "retryable": false,
            "description": "Device has authentication enabled but no password was provided"
        },
        {
            "kind": "partial_failure",
            "exit_code": 4,
            "retryable": false,
            "description": "Operation completed for some devices but failed for others"
        },
        {
            "kind": "conflict",
            "exit_code": 6,
            "retryable": false,
            "description": "Resource already in a conflicting state"
        },
    ])
}

/// Generate a clispec v0.3-compliant machine-readable schema.
pub fn generate_schema() -> Value {
    let cmd = Cli::command();
    let version = cmd.get_version().unwrap_or("unknown");
    let metadata = build_metadata();

    let global_args: Vec<Value> = cmd
        .get_arguments()
        .filter(|a| {
            let id = a.get_id().as_str();
            id != "help" && id != "version"
        })
        .map(arg_to_json)
        .collect();

    let mut commands: Vec<Value> = Vec::new();
    walk_commands(&cmd, "", &metadata, &mut commands);

    let mut schema = json!({
        "clispec": "0.3",
        "name": "shelly",
        "version": version,
        "description": "CLI for managing and controlling Shelly smart home devices over the LAN",
        "global_args": global_args,
        "commands": commands,
        "errors": errors_schema(),
    });
    enrich_v0_3(&mut schema);
    schema
}

fn enrich_v0_3(schema: &mut Value) {
    schema["output"] = json!({"tty":"text","piped":"json"});
    let Some(commands) = schema["commands"].as_array_mut() else {
        return;
    };
    for command in commands {
        let Some(object) = command.as_object_mut() else {
            continue;
        };
        let name = object["name"].as_str().unwrap_or_default().to_string();
        if name == "backup" {
            object.insert("mutating".into(), json!(true));
        }
        let mutating = object["mutating"].as_bool().unwrap_or(false);
        object.insert(
            "effects".into(),
            json!(if !mutating {
                "read_only"
            } else if name.contains("toggle") {
                "non_idempotent"
            } else {
                "idempotent"
            }),
        );
        if name == "completions" {
            object.remove("output_fields");
            object.insert("output_kind".into(), json!("opaque"));
            object.insert("media_type".into(), json!("text/plain"));
            continue;
        }
        if name == "watch" {
            object.insert("output_kind".into(), json!("stream"));
            object.insert("stream_format".into(), json!("terminal"));
            continue;
        }
        let unbounded = matches!(name.as_str(), "devices" | "schedule list" | "webhook list");
        object.insert(
            "cardinality".into(),
            json!(if unbounded { "unbounded" } else { "bounded" }),
        );
        if unbounded {
            object.insert(
                "pagination".into(),
                json!({"style":"offset","limit_arg":"--limit","offset_arg":"--offset"}),
            );
            object.insert("fields_arg".into(), json!("--fields"));
        }
        if name == "capabilities" {
            object.insert("example".into(), json!({"args":["capabilities"]}));
        }
        if name == "schema" {
            object.remove("output_fields");
            object.insert("cardinality".into(), json!("single"));
            object.insert(
                "stdout_schema".into(),
                json!({"$ref":"https://clispec.dev/schema/v0.3.json"}),
            );
        }
        if matches!(name.as_str(), "restore" | "rename" | "reboot") {
            object.insert("confirmation_bypass_arg".into(), json!("--yes"));
        }
        if let Some(fields) = object
            .get_mut("output_fields")
            .and_then(Value::as_array_mut)
        {
            for field in fields {
                let Some(field) = field.as_object_mut() else {
                    continue;
                };
                let kind = field
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("string")
                    .to_string();
                if let Some(base) = kind.strip_suffix(" | null") {
                    field.insert("type".into(), json!(base));
                    field.insert("nullable".into(), json!(true));
                }
                if field.get("type").and_then(Value::as_str) == Some("array")
                    && !field.contains_key("items")
                {
                    field.insert("items".into(), json!({"type":"object"}));
                }
            }
        }
        if !object.contains_key("output_fields") && !object.contains_key("stdout_schema") {
            object.insert("stdout_schema".into(), json!({}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::generate_schema;

    fn find_command<'a>(
        commands: &'a [serde_json::Value],
        name: &str,
    ) -> Option<&'a serde_json::Value> {
        commands
            .iter()
            .find(|c| c["name"] == serde_json::json!(name))
    }

    #[test]
    fn schema_has_required_top_level_fields() {
        let schema = generate_schema();
        assert!(schema["name"].is_string(), "name must be present");
        assert!(schema["version"].is_string(), "version must be present");
        assert!(schema["commands"].is_array(), "commands must be an array");
        assert!(
            schema["global_args"].is_array(),
            "global_args must be an array"
        );
        assert!(schema["errors"].is_array(), "errors must be an array");
        assert_eq!(
            schema["clispec"],
            serde_json::json!("0.3"),
            "clispec version must be 0.3"
        );
    }

    #[test]
    fn schema_name_is_shelly() {
        let schema = generate_schema();
        assert_eq!(schema["name"], serde_json::json!("shelly"));
    }

    #[test]
    fn light_mutating_commands_are_marked() {
        let schema = generate_schema();
        let commands = schema["commands"].as_array().unwrap();

        for name in ["light on", "light off", "light toggle", "light set"] {
            let cmd = find_command(commands, name).unwrap_or_else(|| panic!("{name} not found"));
            assert_eq!(
                cmd["mutating"],
                serde_json::json!(true),
                "{name} should be mutating"
            );
        }

        let cmd = find_command(commands, "light status").expect("light status not found");
        assert_eq!(
            cmd["mutating"],
            serde_json::json!(false),
            "light status should be read-only"
        );
    }

    #[test]
    fn all_commands_have_mutating_field() {
        let schema = generate_schema();
        let commands = schema["commands"].as_array().unwrap();
        for cmd in commands {
            let name = cmd["name"].as_str().unwrap_or("?");
            assert!(
                cmd.get("mutating").is_some(),
                "command '{name}' is missing the mutating field"
            );
        }
    }

    #[test]
    fn errors_array_has_required_fields() {
        let schema = generate_schema();
        let errors = schema["errors"].as_array().unwrap();
        assert!(!errors.is_empty(), "errors array must not be empty");
        for err in errors {
            let kind = err["kind"].as_str().unwrap_or("?");
            assert!(
                err["exit_code"].is_number(),
                "error kind '{kind}' missing exit_code"
            );
            assert!(
                err["retryable"].is_boolean(),
                "error kind '{kind}' missing retryable"
            );
        }
    }

    #[test]
    fn confirmation_required_kind_is_declared() {
        let schema = generate_schema();
        let errors = schema["errors"].as_array().unwrap();
        let found = errors
            .iter()
            .any(|e| e["kind"] == serde_json::json!("confirmation_required"));
        assert!(
            found,
            "confirmation_required must be declared in errors array"
        );
    }

    #[test]
    fn conflict_kind_is_declared() {
        let schema = generate_schema();
        let errors = schema["errors"].as_array().unwrap();
        let found = errors
            .iter()
            .any(|e| e["kind"] == serde_json::json!("conflict"));
        assert!(found, "conflict must be declared in errors array");
    }

    #[test]
    fn list_commands_have_limit_and_offset_args() {
        let schema = generate_schema();
        let commands = schema["commands"].as_array().unwrap();

        for cmd_name in ["devices", "schedule list", "webhook list", "group list"] {
            let cmd =
                find_command(commands, cmd_name).unwrap_or_else(|| panic!("{cmd_name} not found"));
            let args = cmd["args"].as_array().unwrap();
            let arg_names: Vec<_> = args.iter().filter_map(|a| a["name"].as_str()).collect();
            assert!(
                arg_names.contains(&"--limit"),
                "{cmd_name} should have --limit arg"
            );
            assert!(
                arg_names.contains(&"--offset"),
                "{cmd_name} should have --offset arg"
            );
            assert!(
                arg_names.contains(&"--fields"),
                "{cmd_name} should have --fields arg"
            );
        }
    }

    #[test]
    fn output_fields_are_declared_for_key_commands() {
        let schema = generate_schema();
        let commands = schema["commands"].as_array().unwrap();

        for cmd_name in ["devices", "status", "power", "energy", "switch status"] {
            let cmd =
                find_command(commands, cmd_name).unwrap_or_else(|| panic!("{cmd_name} not found"));
            assert!(
                cmd.get("output_fields").is_some(),
                "{cmd_name} should have output_fields declared"
            );
        }
    }

    #[test]
    fn global_args_includes_output_flag() {
        let schema = generate_schema();
        let global_args = schema["global_args"].as_array().unwrap();
        let names: Vec<_> = global_args
            .iter()
            .filter_map(|a| a["name"].as_str())
            .collect();
        assert!(
            names.contains(&"--output"),
            "global_args must include --output"
        );
    }

    #[test]
    fn yes_flag_on_destructive_commands() {
        let schema = generate_schema();
        let commands = schema["commands"].as_array().unwrap();

        for cmd_name in [
            "reboot",
            "restore",
            "rename",
            "firmware update",
            "group remove",
        ] {
            let cmd =
                find_command(commands, cmd_name).unwrap_or_else(|| panic!("{cmd_name} not found"));
            let args = cmd["args"].as_array().unwrap();
            let has_yes = args.iter().any(|a| a["name"] == serde_json::json!("--yes"));
            assert!(has_yes, "{cmd_name} should have a --yes flag");
        }
    }
}
