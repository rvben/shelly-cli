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

    let file: GroupsFile =
        toml::from_str(&data).with_context(|| format!("failed to parse {}", path.display()))?;
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

/// Resolve a group definition against a provided device list.
pub fn resolve_group_with_devices(
    group_def: &GroupDef,
    all_devices: &[DeviceInfo],
) -> Vec<DeviceInfo> {
    match group_def {
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
        GroupDef::Filter { filter } => all_devices
            .iter()
            .filter(|d| matches_filter(d, filter))
            .cloned()
            .collect(),
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

    let matched = resolve_group_with_devices(group_def, &all_devices);

    if matched.is_empty() {
        anyhow::bail!("group '{group_name}' matched no devices");
    }

    Ok(matched)
}

/// Serializable wrapper matching the TOML file structure.
#[derive(Serialize, Deserialize)]
struct GroupsFile {
    #[serde(default)]
    groups: HashMap<String, GroupDef>,
}

pub fn add_group(name: &str, device_names: Vec<String>) -> Result<()> {
    let mut groups = load_groups()?;
    if groups.contains_key(name) {
        anyhow::bail!("group '{name}' already exists. Remove it first to redefine.");
    }
    groups.insert(name.to_string(), GroupDef::Names(device_names));

    let file = GroupsFile { groups };
    let data = toml::to_string_pretty(&file).context("failed to serialize groups")?;
    std::fs::write(groups_path()?, data)?;
    Ok(())
}

pub fn remove_group(name: &str) -> Result<()> {
    let mut groups = load_groups()?;
    if groups.remove(name).is_none() {
        anyhow::bail!("group '{name}' not found");
    }

    let file = GroupsFile { groups };
    let data = toml::to_string_pretty(&file).context("failed to serialize groups")?;
    std::fs::write(groups_path()?, data)?;
    Ok(())
}

pub fn show_group(name: &str, json: bool) -> Result<()> {
    let groups = load_groups()?;
    let group_def = groups
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("group '{name}' not found"))?;

    let all_devices = cache::load_devices()?;
    let matched = resolve_group_with_devices(group_def, &all_devices);

    if json {
        let entries: Vec<serde_json::Value> = matched
            .iter()
            .map(|d| {
                serde_json::json!({
                    "name": d.display_name(),
                    "ip": d.ip.to_string(),
                    "model": d.model,
                    "generation": d.generation.to_string(),
                })
            })
            .collect();
        crate::output::print_json_success(&entries);
    } else if matched.is_empty() {
        println!("Group '{name}' matches no cached devices.");
    } else {
        println!("Group '{name}' ({} devices):", matched.len());
        for d in &matched {
            println!("  {} ({})", d.display_name(), d.ip);
        }
    }

    Ok(())
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
            GroupDef::Filter { filter } => all_devices
                .iter()
                .filter(|d| matches_filter(d, filter))
                .count(),
        };

        let desc = match def {
            GroupDef::Names(names) => names.join(", "),
            GroupDef::Filter { filter } => format!("filter: {filter}"),
        };

        println!("{name:<20} ({count} devices) — {desc}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DeviceGeneration;

    fn test_device(name: &str, id: &str, generation: DeviceGeneration, model: &str) -> DeviceInfo {
        DeviceInfo {
            ip: "10.10.20.1".parse().unwrap(),
            name: Some(name.to_string()),
            id: id.to_string(),
            mac: "AABBCCDDEEFF".to_string(),
            model: model.to_string(),
            generation,
            firmware_version: "1.0.0".to_string(),
            auth_enabled: false,
            num_outputs: 1,
            num_meters: 1,
            app: None,
            device_type: None,
        }
    }

    fn sample_devices() -> Vec<DeviceInfo> {
        vec![
            test_device(
                "Kitchen Light",
                "shellypm-001",
                DeviceGeneration::Gen1,
                "SHSW-PM",
            ),
            test_device(
                "Living Room",
                "shellyplus1pm-002",
                DeviceGeneration::Gen2,
                "SNSW-001P16EU",
            ),
            test_device(
                "Bedroom Fan",
                "shelly1minig3-003",
                DeviceGeneration::Gen3,
                "S3SW-001X8EU",
            ),
            test_device(
                "Garage Plug",
                "shellyplug-004",
                DeviceGeneration::Gen1,
                "SHPLG-1",
            ),
        ]
    }

    #[test]
    fn names_group_all_found() {
        let devices = sample_devices();
        let group = GroupDef::Names(vec!["Kitchen Light".to_string(), "Living Room".to_string()]);
        let result = resolve_group_with_devices(&group, &devices);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "shellypm-001");
        assert_eq!(result[1].id, "shellyplus1pm-002");
    }

    #[test]
    fn names_group_missing_device() {
        let devices = sample_devices();
        let group = GroupDef::Names(vec![
            "Kitchen Light".to_string(),
            "Nonexistent Device".to_string(),
        ]);
        let result = resolve_group_with_devices(&group, &devices);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "shellypm-001");
    }

    #[test]
    fn filter_gen1() {
        let devices = sample_devices();
        let group = GroupDef::Filter {
            filter: "gen1".to_string(),
        };
        let result = resolve_group_with_devices(&group, &devices);
        assert_eq!(result.len(), 2);
        assert!(
            result
                .iter()
                .all(|d| d.generation == DeviceGeneration::Gen1)
        );
    }

    #[test]
    fn filter_gen2() {
        let devices = sample_devices();
        let group = GroupDef::Filter {
            filter: "gen2".to_string(),
        };
        let result = resolve_group_with_devices(&group, &devices);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].generation, DeviceGeneration::Gen2);
    }

    #[test]
    fn filter_gen3() {
        let devices = sample_devices();
        let group = GroupDef::Filter {
            filter: "gen3".to_string(),
        };
        let result = resolve_group_with_devices(&group, &devices);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].generation, DeviceGeneration::Gen3);
    }

    #[test]
    fn filter_all() {
        let devices = sample_devices();
        let group = GroupDef::Filter {
            filter: "all".to_string(),
        };
        let result = resolve_group_with_devices(&group, &devices);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn filter_by_model_substring() {
        let devices = sample_devices();
        let group = GroupDef::Filter {
            filter: "SHSW".to_string(),
        };
        let result = resolve_group_with_devices(&group, &devices);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].model, "SHSW-PM");
    }

    #[test]
    fn filter_by_name_substring() {
        let devices = sample_devices();
        let group = GroupDef::Filter {
            filter: "light".to_string(),
        };
        let result = resolve_group_with_devices(&group, &devices);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "shellypm-001");
    }

    #[test]
    fn empty_device_list() {
        let devices: Vec<DeviceInfo> = Vec::new();
        let group = GroupDef::Filter {
            filter: "all".to_string(),
        };
        let result = resolve_group_with_devices(&group, &devices);
        assert!(result.is_empty());
    }

    #[test]
    fn names_group_empty_device_list() {
        let devices: Vec<DeviceInfo> = Vec::new();
        let group = GroupDef::Names(vec!["Kitchen Light".to_string()]);
        let result = resolve_group_with_devices(&group, &devices);
        assert!(result.is_empty());
    }

    #[test]
    fn filter_case_insensitive() {
        let devices = sample_devices();
        let group = GroupDef::Filter {
            filter: "GEN1".to_string(),
        };
        let result = resolve_group_with_devices(&group, &devices);
        assert_eq!(result.len(), 2);
    }
}
