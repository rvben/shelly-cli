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
