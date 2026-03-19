use std::path::PathBuf;

use anyhow::Result;

use crate::model::DeviceInfo;

fn cache_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine config directory"))?
        .join("shelly-cli");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("devices.json"))
}

pub fn load_devices() -> Result<Vec<DeviceInfo>> {
    let path = cache_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(&path)?;
    let devices: Vec<DeviceInfo> = serde_json::from_str(&data)?;
    Ok(devices)
}

pub fn save_devices(devices: &[DeviceInfo]) -> Result<()> {
    let path = cache_path()?;
    let data = serde_json::to_string_pretty(devices)?;
    std::fs::write(&path, data)?;
    Ok(())
}

pub fn find_device_by_name(devices: &[DeviceInfo], name: &str) -> Option<DeviceInfo> {
    let name_lower = name.to_lowercase();
    devices.iter().find(|d| {
        d.display_name().to_lowercase() == name_lower
            || d.id.to_lowercase() == name_lower
            || d.display_name().to_lowercase().contains(&name_lower)
    }).cloned()
}

/// Find a device by name, returning helpful suggestions on failure.
pub fn find_device_by_name_with_suggestions(devices: &[DeviceInfo], name: &str) -> Result<DeviceInfo> {
    if let Some(device) = find_device_by_name(devices, name) {
        return Ok(device);
    }

    let name_lower = name.to_lowercase();

    let mut candidates: Vec<(&str, f64)> = devices
        .iter()
        .flat_map(|d| {
            let display = d.display_name();
            let mut entries = vec![
                (display, strsim::normalized_damerau_levenshtein(&name_lower, &display.to_lowercase())),
            ];
            if d.id != display {
                entries.push((
                    d.id.as_str(),
                    strsim::normalized_damerau_levenshtein(&name_lower, &d.id.to_lowercase()),
                ));
            }
            entries
        })
        .filter(|(_, score)| *score > 0.4)
        .collect();

    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    candidates.dedup_by(|a, b| a.0 == b.0);
    candidates.truncate(3);

    if candidates.is_empty() {
        anyhow::bail!("device '{name}' not found in cache. Run 'shelly discover' first.");
    }

    let suggestions: Vec<String> = candidates.iter().map(|(name, _)| format!("  {name}")).collect();
    anyhow::bail!(
        "device '{name}' not found. Did you mean:\n{}",
        suggestions.join("\n")
    );
}
