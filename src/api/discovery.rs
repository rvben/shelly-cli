use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use anyhow::Result;
use ipnet::Ipv4Net;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::model::DeviceInfo;

use super::probe_device;

pub async fn scan_subnet(
    subnet: Ipv4Net,
    http_timeout: Duration,
    on_found: impl Fn(&DeviceInfo),
) -> Result<Vec<DeviceInfo>> {
    let client = reqwest::Client::builder()
        .timeout(http_timeout)
        .build()?;

    let (tx, mut rx) = mpsc::channel::<DeviceInfo>(64);

    let hosts: Vec<Ipv4Addr> = subnet.hosts().collect();
    for chunk in hosts.chunks(32) {
        let mut handles = Vec::new();
        for &ip in chunk {
            let client = client.clone();
            let tx = tx.clone();
            handles.push(tokio::spawn(async move {
                let addr = IpAddr::V4(ip);
                if let Ok(Ok(info)) =
                    timeout(http_timeout, probe_device(addr, &client)).await
                {
                    let _ = tx.send(info).await;
                }
            }));
        }

        for handle in handles {
            let _ = handle.await;
        }
    }

    drop(tx);

    let mut devices = Vec::new();
    while let Some(info) = rx.recv().await {
        on_found(&info);
        devices.push(info);
    }

    devices.sort_by(|a, b| a.ip.to_string().cmp(&b.ip.to_string()));
    Ok(devices)
}

/// Enrich Gen1 devices with their name from /settings
pub async fn enrich_gen1_name(
    info: &mut DeviceInfo,
    client: &reqwest::Client,
) -> Result<()> {
    if info.name.is_some() {
        return Ok(());
    }

    let url = format!("http://{}/settings", info.ip);
    let resp = client.get(&url).send().await?;
    let settings: serde_json::Value = resp.json().await?;

    if let Some(name) = settings.get("name").and_then(|v| v.as_str())
        && !name.is_empty()
    {
        info.name = Some(name.to_string());
    }

    Ok(())
}
