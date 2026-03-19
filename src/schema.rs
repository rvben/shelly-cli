use serde::Serialize;

#[derive(Serialize)]
pub struct SchemaCommand {
    pub description: String,
    pub args: Vec<SchemaArg>,
    pub mutating: bool,
    pub output_fields: Vec<String>,
}

#[derive(Serialize)]
pub struct SchemaArg {
    pub name: String,
    pub description: String,
    pub required: bool,
}

pub fn generate_schema() -> serde_json::Value {
    let mut commands = serde_json::Map::new();

    commands.insert(
        "discover".into(),
        serde_json::to_value(SchemaCommand {
            description: "Scan network for Shelly devices".into(),
            args: vec![SchemaArg {
                name: "--subnet".into(),
                description: "Subnet to scan in CIDR notation (e.g. 10.10.20.0/24)".into(),
                required: false,
            }],
            mutating: false,
            output_fields: vec![
                "name", "ip", "gen", "model", "firmware_version", "mac",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        })
        .unwrap(),
    );

    commands.insert(
        "devices".into(),
        serde_json::to_value(SchemaCommand {
            description: "List known/cached devices".into(),
            args: vec![SchemaArg {
                name: "--refresh".into(),
                description: "Re-scan network before listing".into(),
                required: false,
            }],
            mutating: false,
            output_fields: vec![
                "name", "ip", "gen", "model", "firmware_version", "mac",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        })
        .unwrap(),
    );

    commands.insert(
        "status".into(),
        serde_json::to_value(SchemaCommand {
            description: "Get device status".into(),
            args: vec![
                SchemaArg {
                    name: "--host".into(),
                    description: "Target device by IP".into(),
                    required: false,
                },
                SchemaArg {
                    name: "--name".into(),
                    description: "Target device by name".into(),
                    required: false,
                },
                SchemaArg {
                    name: "--all".into(),
                    description: "Query all known devices".into(),
                    required: false,
                },
            ],
            mutating: false,
            output_fields: vec![
                "switches", "inputs", "wifi", "uptime", "temperature",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        })
        .unwrap(),
    );

    commands.insert(
        "switch status".into(),
        serde_json::to_value(SchemaCommand {
            description: "Get switch/relay status".into(),
            args: vec![
                SchemaArg {
                    name: "--host".into(),
                    description: "Target device by IP".into(),
                    required: false,
                },
                SchemaArg {
                    name: "--id".into(),
                    description: "Switch ID (default: 0)".into(),
                    required: false,
                },
            ],
            mutating: false,
            output_fields: vec![
                "id", "output", "power_watts", "voltage", "current", "total_energy_wh",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        })
        .unwrap(),
    );

    commands.insert(
        "switch on".into(),
        serde_json::to_value(SchemaCommand {
            description: "Turn switch on".into(),
            args: vec![
                SchemaArg {
                    name: "--host".into(),
                    description: "Target device by IP".into(),
                    required: false,
                },
                SchemaArg {
                    name: "--id".into(),
                    description: "Switch ID (default: 0)".into(),
                    required: false,
                },
            ],
            mutating: true,
            output_fields: vec!["was_on".into()],
        })
        .unwrap(),
    );

    commands.insert(
        "switch off".into(),
        serde_json::to_value(SchemaCommand {
            description: "Turn switch off".into(),
            args: vec![
                SchemaArg {
                    name: "--host".into(),
                    description: "Target device by IP".into(),
                    required: false,
                },
                SchemaArg {
                    name: "--id".into(),
                    description: "Switch ID (default: 0)".into(),
                    required: false,
                },
            ],
            mutating: true,
            output_fields: vec!["was_on".into()],
        })
        .unwrap(),
    );

    commands.insert(
        "switch toggle".into(),
        serde_json::to_value(SchemaCommand {
            description: "Toggle switch state".into(),
            args: vec![
                SchemaArg {
                    name: "--host".into(),
                    description: "Target device by IP".into(),
                    required: false,
                },
                SchemaArg {
                    name: "--id".into(),
                    description: "Switch ID (default: 0)".into(),
                    required: false,
                },
            ],
            mutating: true,
            output_fields: vec!["was_on".into()],
        })
        .unwrap(),
    );

    commands.insert(
        "power".into(),
        serde_json::to_value(SchemaCommand {
            description: "Get power/energy readings".into(),
            args: vec![
                SchemaArg {
                    name: "--host".into(),
                    description: "Target device by IP".into(),
                    required: false,
                },
                SchemaArg {
                    name: "--all".into(),
                    description: "Query all known devices".into(),
                    required: false,
                },
            ],
            mutating: false,
            output_fields: vec![
                "power_watts",
                "voltage",
                "current",
                "total_energy_wh",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        })
        .unwrap(),
    );

    commands.insert(
        "firmware check".into(),
        serde_json::to_value(SchemaCommand {
            description: "Check for firmware updates".into(),
            args: vec![
                SchemaArg {
                    name: "--host".into(),
                    description: "Target device by IP".into(),
                    required: false,
                },
                SchemaArg {
                    name: "--all".into(),
                    description: "Check all known devices".into(),
                    required: false,
                },
            ],
            mutating: false,
            output_fields: vec![
                "current_version",
                "has_update",
                "stable_version",
                "beta_version",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        })
        .unwrap(),
    );

    commands.insert(
        "config get".into(),
        serde_json::to_value(SchemaCommand {
            description: "Get device configuration".into(),
            args: vec![SchemaArg {
                name: "--host".into(),
                description: "Target device by IP".into(),
                required: false,
            }],
            mutating: false,
            output_fields: vec!["config".into()],
        })
        .unwrap(),
    );

    commands.insert(
        "reboot".into(),
        serde_json::to_value(SchemaCommand {
            description: "Reboot a device".into(),
            args: vec![SchemaArg {
                name: "--host".into(),
                description: "Target device by IP".into(),
                required: false,
            }],
            mutating: true,
            output_fields: vec!["status".into()],
        })
        .unwrap(),
    );

    serde_json::json!({ "commands": commands })
}
