use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::cache;
use crate::model::DeviceInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GroupDef {
    Names(Vec<String>),
    Filter { filter: String },
}

pub fn groups_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine config directory"))?
        .join("shelly-cli");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("groups.toml"))
}

pub fn load_groups() -> Result<HashMap<String, GroupDef>> {
    let path = groups_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let data = std::fs::read_to_string(&path)?;

    #[derive(Deserialize)]
    struct GroupsFile {
        #[serde(default)]
        groups: HashMap<String, GroupDef>,
    }

    let file: GroupsFile = toml::from_str(&data)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(file.groups)
}

fn matches_filter(device: &DeviceInfo, filter: &str) -> bool {
    let filter_lower = filter.to_lowercase();
    match filter_lower.as_str() {
        "gen1" => device.generation == crate::model::DeviceGeneration::Gen1,
        "gen2" => device.generation == crate::model::DeviceGeneration::Gen2,
        "gen3" => device.generation == crate::model::DeviceGeneration::Gen3,
        "all" => true,
        _ => {
            device.model.to_lowercase().contains(&filter_lower)
                || device.display_name().to_lowercase().contains(&filter_lower)
        }
    }
}

pub fn resolve_group(group_name: &str) -> Result<Vec<DeviceInfo>> {
    let groups = load_groups()?;
    let group_def = groups
        .get(group_name)
        .ok_or_else(|| anyhow::anyhow!("group '{group_name}' not found in groups.toml"))?;

    let all_devices = cache::load_devices()?;
    if all_devices.is_empty() {
        anyhow::bail!("no cached devices. Run 'shelly discover' first.");
    }

    let matched = match group_def {
        GroupDef::Names(names) => {
            let mut result = Vec::new();
            for name in names {
                let name_lower = name.to_lowercase();
                if let Some(device) = all_devices.iter().find(|d| {
                    d.display_name().to_lowercase() == name_lower
                        || d.id.to_lowercase() == name_lower
                        || d.display_name().to_lowercase().contains(&name_lower)
                }) {
                    result.push(device.clone());
                } else {
                    eprintln!("warning: device '{name}' not found in cache");
                }
            }
            result
        }
        GroupDef::Filter { filter } => {
            all_devices
                .into_iter()
                .filter(|d| matches_filter(d, filter))
                .collect()
        }
    };

    if matched.is_empty() {
        anyhow::bail!("group '{group_name}' matched no devices");
    }

    Ok(matched)
}

pub fn list_groups(json: bool) -> Result<()> {
    let groups = load_groups()?;
    if groups.is_empty() {
        if json {
            crate::output::print_json_success(&Vec::<serde_json::Value>::new());
        } else {
            eprintln!("No groups defined. Create groups in ~/.config/shelly-cli/groups.toml");
            eprintln!();
            eprintln!("Example:");
            eprintln!("  [groups]");
            eprintln!("  lights = [\"Family Room Light Switch 1\", \"Frontdoor Light\"]");
            eprintln!("  gen1 = {{ filter = \"gen1\" }}");
        }
        return Ok(());
    }

    let all_devices = cache::load_devices().unwrap_or_default();

    if json {
        let mut entries = Vec::new();
        for (name, def) in &groups {
            let (count, description) = match def {
                GroupDef::Names(names) => (names.len(), names.join(", ")),
                GroupDef::Filter { filter } => {
                    let count = all_devices
                        .iter()
                        .filter(|d| matches_filter(d, filter))
                        .count();
                    (count, format!("filter: {filter}"))
                }
            };
            entries.push(serde_json::json!({
                "name": name,
                "device_count": count,
                "description": description,
            }));
        }
        crate::output::print_json_success(&entries);
        return Ok(());
    }

    for (name, def) in &groups {
        let count = match def {
            GroupDef::Names(names) => names.len(),
            GroupDef::Filter { filter } => {
                all_devices
                    .iter()
                    .filter(|d| matches_filter(d, filter))
                    .count()
            }
        };

        let desc = match def {
            GroupDef::Names(names) => names.join(", "),
            GroupDef::Filter { filter } => format!("filter: {filter}"),
        };

        println!("{name:<20} ({count} devices) — {desc}");
    }

    Ok(())
}
