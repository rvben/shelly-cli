//! Conformance tests for `shelly_core::ShellyClient` as a `switchkit::SmartDevice`.
//!
//! These tests drive the adapter against `httpmock` HTTP servers standing in
//! for real Shelly Gen1/Gen2 devices, and assert on VALUES, not just on
//! success. Each assertion is chosen so a wrong mapping (fabricated
//! metering, an unconverted unit, a guessed vendor, `was_on` instead of a
//! confirmed readback, a percentage invented from a dBm reading, ...) makes
//! the test fail rather than merely pass differently.
//!
//! The httpmock server's ephemeral loopback bind is the test harness, not a
//! fixture address; any IP embedded in a mocked JSON body uses an RFC 5737
//! documentation range instead.

use httpmock::prelude::*;
use shelly_core::ShellyClient;
use switchkit::guardrail::{Hazard, classify};
use switchkit::{DeviceTarget, PowerAction, RelayState, SmartDevice, Vendor};

/// Gen2 metering plug: every telemetry leaf on the snapshot must equal the
/// value actually reported by the device, unit-converted where the device
/// reports Wh and switchkit wants kWh, and left absent where Shelly's status
/// response has no such field (`energy.today_kwh`).
#[tokio::test]
async fn gen2_metering_plug_maps_every_field_honestly() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/shelly");
            then.status(200).json_body(serde_json::json!({
                "id": "shellyplus1pm-aabbccddeeff",
                "mac": "AABBCCDDEEFF",
                "model": "SNSW-001P16EU",
                "gen": 2,
                "ver": "1.2.3",
                "app": "Plus1PM",
                "auth_en": false
            }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/rpc/Shelly.GetStatus");
            then.status(200).json_body(serde_json::json!({
                "switch:0": {
                    "id": 0,
                    "output": true,
                    "apower": 42.5,
                    "voltage": 230.0,
                    "current": 0.18,
                    "aenergy": { "total": 12300.0 }
                },
                "wifi": { "rssi": -58 }
            }));
        })
        .await;

    let client = ShellyClient::default();
    let target = DeviceTarget::new(server.address().to_string());
    let snapshot = client
        .status(&target)
        .await
        .expect("status should succeed against the mock");

    assert_eq!(snapshot.relays.len(), 1);
    assert_eq!(snapshot.relays[0].state, RelayState::On);

    let energy = snapshot
        .energy
        .expect("a metering switch must produce Some(Energy)");
    assert_eq!(energy.power_w, Some(42.5));
    assert_eq!(energy.voltage_v, Some(230.0));
    assert_eq!(energy.current_a, Some(0.18));
    assert_eq!(
        energy.total_kwh,
        Some(12.3),
        "12300 Wh must convert to 12.3 kWh, not pass through as raw Wh"
    );
    assert_eq!(
        energy.today_kwh, None,
        "Shelly's status response has no daily counter; must not be fabricated"
    );

    let signal = snapshot
        .signal
        .expect("a real rssi reading must produce Some(Signal)");
    assert_eq!(signal.rssi_dbm, Some(-58));
    assert_eq!(
        signal.quality_percent, None,
        "must never invent a percentage from a dBm reading"
    );

    assert!(snapshot.capabilities.metering);
    assert!(snapshot.capabilities.console);
    assert_eq!(
        snapshot.firmware.and_then(|f| f.version).as_deref(),
        Some("1.2.3")
    );
}

/// `set_power` must return the CONFIRMED post-change state read back via
/// `Switch.GetStatus`, never `SwitchResult.was_on` (Gen2's `Switch.Set`
/// reports the PREVIOUS state under that key). The mock deliberately makes
/// the two disagree (`was_on: false` vs. a readback of `output: true`) so
/// this fails if the adapter ever regresses to trusting `was_on`.
#[tokio::test]
async fn set_power_returns_confirmed_readback_not_was_on() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/shelly");
            then.status(200).json_body(serde_json::json!({
                "id": "shellyplus1-aabbccddeeff",
                "mac": "AABBCCDDEEFF",
                "model": "SNSW-001X16EU",
                "gen": 2,
                "ver": "1.0.0"
            }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/rpc/Switch.Set");
            then.status(200)
                .json_body(serde_json::json!({ "was_on": false }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/rpc/Switch.GetStatus");
            then.status(200)
                .json_body(serde_json::json!({ "id": 0, "output": true }));
        })
        .await;

    let client = ShellyClient::default();
    let target = DeviceTarget::new(server.address().to_string());
    let relay = client
        .set_power(&target, Some(0), PowerAction::On)
        .await
        .expect("set_power should succeed against the mock");

    assert_eq!(
        relay.state,
        RelayState::On,
        "must report the confirmed readback (output: true), not was_on (false)"
    );
}

/// A Gen2 switch reporting NO power/energy fields must yield `energy ==
/// None`, never a zeroed `Energy` fabricated from `PowerReading`-style
/// defaults.
#[tokio::test]
async fn gen2_switch_without_metering_yields_no_energy() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/shelly");
            then.status(200).json_body(serde_json::json!({
                "id": "shellyplus1-aabbccddeeff",
                "mac": "AABBCCDDEEFF",
                "model": "SNSW-001X16EU",
                "gen": 2,
                "ver": "1.0.0"
            }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/rpc/Shelly.GetStatus");
            then.status(200).json_body(serde_json::json!({
                "switch:0": { "id": 0, "output": true }
            }));
        })
        .await;

    let client = ShellyClient::default();
    let target = DeviceTarget::new(server.address().to_string());
    let snapshot = client
        .status(&target)
        .await
        .expect("status should succeed against the mock");

    assert_eq!(snapshot.relays.len(), 1);
    assert_eq!(snapshot.relays[0].state, RelayState::On);
    assert_eq!(
        snapshot.energy, None,
        "a non-metering switch must not produce a zeroed Energy"
    );
    assert!(!snapshot.capabilities.metering);
}

/// Gen1 device: `console` capability must be false (Gen1 has no RPC
/// console), the relay must be mapped from the Gen1 relay state, and signal
/// from the Gen1 `wifi_sta` rssi reading.
#[tokio::test]
async fn gen1_device_maps_relay_and_signal_console_false() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/shelly");
            then.status(200).json_body(serde_json::json!({
                "type": "SHSW-1",
                "mac": "112233445566",
                "auth": false,
                "fw": "20230913-114003/v1.14.0-gcb84623",
                "num_outputs": 1,
                "num_meters": 0
            }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/status");
            then.status(200).json_body(serde_json::json!({
                "relays": [
                    { "ison": true, "source": "http", "has_timer": false }
                ],
                "wifi_sta": {
                    "connected": true,
                    "ssid": "TestNet",
                    "ip": "198.51.100.5",
                    "rssi": -60
                }
            }));
        })
        .await;

    let client = ShellyClient::default();
    let target = DeviceTarget::new(server.address().to_string());
    let snapshot = client
        .status(&target)
        .await
        .expect("status should succeed against the mock");

    assert!(!snapshot.capabilities.console, "Gen1 has no RPC console");
    assert_eq!(snapshot.relays.len(), 1);
    assert_eq!(snapshot.relays[0].state, RelayState::On);

    let signal = snapshot
        .signal
        .expect("a real Gen1 rssi reading must produce Some(Signal)");
    assert_eq!(signal.rssi_dbm, Some(-60));
    assert_eq!(signal.quality_percent, None);
}

/// A reachable host that answers `/shelly` with a body describing neither a
/// Gen1 nor Gen2 device must probe as `Ok(None)`, never a guessed vendor.
#[tokio::test]
async fn probe_reachable_non_shelly_returns_ok_none() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/shelly");
            then.status(200)
                .json_body(serde_json::json!({ "hello": "world" }));
        })
        .await;

    let client = ShellyClient::default();
    let target = DeviceTarget::new(server.address().to_string());
    let result = client.probe(&target).await;

    assert!(
        matches!(result, Ok(None)),
        "a non-Shelly response must probe as Ok(None), got {result:?}"
    );
}

/// An unreachable target (connection refused, nothing listening) must fail
/// `status()` with `Error::Network`, never a fabricated snapshot.
#[tokio::test]
async fn status_offline_target_returns_network_error() {
    let client = ShellyClient::default();
    let target = DeviceTarget::new("127.0.0.1:1".to_string());

    let err = client
        .status(&target)
        .await
        .expect_err("an unreachable target must not produce a snapshot");

    assert!(
        matches!(err, switchkit::Error::Network { .. }),
        "expected Error::Network, got {err:?}"
    );
}

// Sentinel-not-fabricated coverage (a `/shelly` response that omits
// model/firmware must map to `None`, never the `"unknown"` sentinel
// `DeviceInfo` falls back to internally) is already proven by
// `snapshot_omits_sentinel_model_and_firmware` in
// `core/src/switchkit_impl.rs`'s own test module; not duplicated here.

/// Guardrail parity: `shelly-core` has no independent destructive-command
/// table of its own. `ShellyClient::console` (see its doc comment in
/// `core/src/switchkit_impl.rs`) passes the RPC method straight through and
/// states explicitly that "the CALLER classifies it via `guardrail::classify`
/// first" - shelly-core defers entirely to `switchkit::guardrail`. This test
/// therefore asserts the classification directly against
/// `switchkit::guardrail` for a representative Shelly command set, rather
/// than comparing two tables. (`cli/src/errors.rs::check_confirmation` is a
/// separate, CLI-level gate over a handful of high-level actions such as
/// "reboot device(s)" or "rename device", not over raw RPC method strings, so
/// there is no local method-hazard table in this codebase to compare
/// against.)
#[test]
fn guardrail_classifies_shelly_commands_by_hazard() {
    assert!(
        matches!(
            classify(Vendor::Shelly, "Shelly.FactoryReset"),
            Hazard::Destructive(_)
        ),
        "a factory reset must be classified destructive"
    );
    assert!(
        matches!(
            classify(Vendor::Shelly, "Shelly.Update"),
            Hazard::Destructive(_)
        ),
        "a firmware update must be classified destructive"
    );
    assert!(
        matches!(
            classify(Vendor::Shelly, "Sys.SetConfig"),
            Hazard::Destructive(_)
        ),
        "a config write must be classified destructive"
    );
    assert_eq!(
        classify(Vendor::Shelly, "Shelly.GetStatus"),
        Hazard::Safe,
        "a getter must be classified safe"
    );
    assert_eq!(
        classify(Vendor::Shelly, "Switch.Set"),
        Hazard::Safe,
        "basic reversible relay control must be classified safe"
    );
}
