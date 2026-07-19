# shelly-core

Rust library for talking to Shelly smart-home devices: Gen1 (REST) and
Gen2/Gen3 (RPC over HTTP) discovery, status, and control, plus the shared
device model types.

Implements the [`switchkit`](https://crates.io/crates/switchkit)
`SmartDevice` trait via `ShellyClient`, so a Shelly device can be driven
through switchkit's generic device abstraction alongside other smart-device
backends.

## Features

- Device discovery and probing (`probe_device`, `probe_target`, `scan_subnet`)
- Gen1 and Gen2/3 HTTP + RPC clients (`Gen1Device`, `Gen2Device`)
- Shared model types: `DeviceInfo`, `DeviceStatus`, `DeviceGeneration`,
  `SwitchStatus`, `LightStatus`, `LightParams`, `PowerReading`, and more
- `switchkit::SmartDevice` adapter (`ShellyClient`)

This crate is the library backing the [`shelly-cli`](https://crates.io/crates/shelly-cli)
command-line tool. It has no CLI of its own.

## License

MIT
