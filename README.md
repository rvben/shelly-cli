# shelly

A fast CLI for discovering, monitoring, and controlling Shelly smart home devices.

![CI](https://github.com/rvben/shelly-cli/actions/workflows/ci.yml/badge.svg) [![crates.io](https://img.shields.io/crates/v/shelly-cli.svg)](https://crates.io/crates/shelly-cli) [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## Features

- Auto-discovery of Shelly devices on the local network
- Unified Gen1 + Gen2/Gen3 support (transparent protocol handling)
- Interactive watch dashboard with live power, temperature, WiFi monitoring -- and switch control
- Device health checks (temperature, WiFi signal, firmware, uptime)
- Device groups with filter-based and name-based definitions
- Firmware check and update across all devices
- Device renaming from the CLI
- Structured JSON output for scripting and AI agent integration
- Shell completions (bash, zsh, fish, powershell)
- Fuzzy device name matching with "did you mean?" suggestions
- Color output with automatic detection

## Install

### From source (cargo)

```bash
cargo install shelly-cli
```

### Homebrew (macOS/Linux)

```bash
brew install rvben/tap/shelly-cli
```

### Pre-built binaries

Download from [GitHub Releases](https://github.com/rvben/shelly-cli/releases).

### From source (git)

```bash
git clone https://github.com/rvben/shelly-cli.git
cd shelly-cli
cargo install --path .
```

## Quick Start

```bash
# 1. Discover devices on your network
shelly discover --subnet 192.168.1.0/24

# 2. See what you found
shelly devices

# 3. Check device health
shelly health
```

## Usage

### Device control

```bash
shelly on "Kitchen Light"          # Turn on
shelly off "Kitchen Light"         # Turn off
shelly toggle -n "Living Room"     # Toggle
shelly status -n "Kitchen Light"   # Get status
```

### Monitoring

```bash
shelly watch                       # Interactive dashboard
shelly health                      # Health check all devices
shelly power -a                    # Power usage for all devices
```

### Device management

```bash
shelly rename -n "old-name" "New Name"    # Rename device
shelly firmware check -a                   # Check for updates
shelly firmware update -a                  # Update all firmware
shelly reboot -n "Kitchen Light"          # Reboot device
```

### Groups

```bash
shelly group add lights "Kitchen" "Living Room" "Bedroom"
shelly group list
shelly -g lights off               # Turn off all lights
shelly -g lights status            # Status of all lights
```

### Configuration

```bash
shelly config get -n "Kitchen"     # Get device config
shelly completions zsh             # Generate shell completions
```

## Agent Integration

Designed for scripting and AI agent use with structured, machine-readable output.

```bash
# Structured JSON output (auto-enabled when piped)
shelly status -a | jq '.data'

# Consistent envelope: {"ok": true, "data": ...} or {"ok": false, "error": {...}}
shelly -n "nonexistent" status
# {"ok": false, "error": {"code": "DEVICE_NOT_FOUND", "message": "..."}}

# Machine-readable schema
shelly schema
```

## Groups Configuration

Groups are defined in a TOML file:

```toml
# ~/.config/shelly-cli/groups.toml (Linux)
# ~/Library/Application Support/shelly-cli/groups.toml (macOS)

[groups]
lights = ["Kitchen Light", "Living Room Light", "Bedroom Light"]
gen1 = { filter = "gen1" }
gen3 = { filter = "gen3" }
all = { filter = "all" }
```

Or manage via CLI: `shelly group add`, `shelly group remove`, `shelly group show`.

## Supported Devices

| Generation | Examples | Status |
|---|---|---|
| Gen1 | Shelly 1, 1PM, 2.5, Plug S, Dimmer | Supported (switch, power, firmware) |
| Gen2 | Shelly Plus 1, Plus 1PM, Plus 2PM | Supported (switch, power, firmware) |
| Gen3 | Shelly Mini 1PM G3, Plus series G3 | Supported (switch, power, firmware) |

## License

MIT License -- see [LICENSE](LICENSE) file.
