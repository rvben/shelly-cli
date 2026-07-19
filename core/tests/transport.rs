//! Transport-classification tests for `shelly-core`.
//!
//! These exercise the typed `Error` at every choke point: the Gen1/Gen2 HTTP
//! choke points (`get_json`/`rpc_call`) and the standalone `probe_target`
//! path, which has its own direct `/shelly` fetch. All servers are
//! `httpmock`'s ephemeral loopback bind (a test harness, not a fixture
//! address); `DeviceInfo.ip` in fixtures uses an RFC 5737 documentation
//! address since it is never dialed (only `base_host` is).

use httpmock::prelude::*;

use shelly_core::{
    DeviceGeneration, DeviceInfo, Error, Gen1Device, Gen2Device, create_device_with_host,
    probe_target,
};

fn test_device_info(generation: DeviceGeneration) -> DeviceInfo {
    DeviceInfo {
        ip: "192.0.2.1".parse().unwrap(),
        name: None,
        id: "test-device".to_string(),
        mac: "AABBCCDDEEFF".to_string(),
        model: "TEST".to_string(),
        generation,
        firmware_version: "1.0.0".to_string(),
        auth_enabled: false,
        num_outputs: 1,
        num_meters: 1,
        app: None,
        device_type: None,
    }
}

fn gen2_shelly_body() -> serde_json::Value {
    serde_json::json!({
        "id": "shellyplus1pm-aabbccddeeff",
        "mac": "AABBCCDDEEFF",
        "model": "SNSW-001P16EU",
        "gen": 2,
        "fw_id": "20230913-114003",
        "ver": "1.0.0",
        "app": "Plus1PM",
        "auth_en": false,
        "name": null
    })
}

#[tokio::test]
async fn gen1_401_maps_to_auth_error() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/status");
            then.status(401).body("unauthorized");
        })
        .await;

    let device = Gen1Device::new_with_host(
        test_device_info(DeviceGeneration::Gen1),
        server.address().to_string(),
        reqwest::Client::new(),
        None,
    );

    let err = device.status().await.expect_err("expected an error");
    assert!(
        matches!(err, Error::Auth { .. }),
        "expected Error::Auth, got {err:?}"
    );
}

#[tokio::test]
async fn gen2_rpc_error_body_maps_to_rejected() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/rpc/Shelly.Reboot");
            then.status(200).json_body(serde_json::json!({
                "error": { "code": -103, "message": "invalid argument" }
            }));
        })
        .await;

    let device = Gen2Device::new_with_host(
        test_device_info(DeviceGeneration::Gen2),
        server.address().to_string(),
        reqwest::Client::new(),
        None,
    );

    let err = device.reboot().await.expect_err("expected an error");
    assert!(
        matches!(err, Error::Rejected { .. }),
        "expected Error::Rejected, got {err:?}"
    );
}

#[tokio::test]
async fn connection_refused_maps_to_network_error() {
    // Port 1 on loopback is a privileged, essentially-never-listening port;
    // nothing here is dialed except the local machine's own network stack.
    let client = reqwest::Client::new();

    let err = probe_target("127.0.0.1:1", &client)
        .await
        .expect_err("expected an error");
    assert!(
        matches!(err, Error::Network { .. }),
        "expected Error::Network, got {err:?}"
    );
}

#[tokio::test]
async fn probe_target_non_shelly_body_maps_to_parse() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/shelly");
            then.status(200)
                .json_body(serde_json::json!({ "hello": "world" }));
        })
        .await;

    let client = reqwest::Client::new();
    let host = server.address().to_string();

    let err = probe_target(&host, &client)
        .await
        .expect_err("expected an error");
    assert!(
        matches!(err, Error::Parse { .. }),
        "expected Error::Parse, got {err:?}"
    );
}

#[tokio::test]
async fn probe_target_reaches_gen2_mock_via_host_port() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/shelly");
            then.status(200).json_body(gen2_shelly_body());
        })
        .await;

    let client = reqwest::Client::new();
    let host = server.address().to_string();

    let info = probe_target(&host, &client)
        .await
        .expect("expected Ok(DeviceInfo), proving host:port addressing reached the mock");

    assert_eq!(info.generation, DeviceGeneration::Gen2);
    assert_eq!(info.model, "SNSW-001P16EU");
    assert_eq!(info.mac, "AABBCCDDEEFF");
    // The IP was derived from the mock server's own loopback address, not a
    // fixture default, proving host:port (not just host) was honored.
    assert_eq!(info.ip, server.address().ip());
}

#[tokio::test]
async fn create_device_with_host_reaches_mock_server() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/rpc/Shelly.GetStatus");
            then.status(200).json_body(serde_json::json!({
                "switch:0": { "id": 0, "output": true }
            }));
        })
        .await;

    let device = create_device_with_host(
        test_device_info(DeviceGeneration::Gen2),
        server.address().to_string(),
        reqwest::Client::new(),
        None,
    );

    // create_device (no explicit host) would build the URL from info.ip
    // (192.0.2.1, an address that is never dialed in this test), so a
    // successful call here proves create_device_with_host actually used
    // base_host to reach the mock rather than falling back to info.ip.
    device
        .status()
        .await
        .expect("expected create_device_with_host to reach the mock via base_host");
}
