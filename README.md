# shelly

A fast CLI for discovering, monitoring, and controlling Shelly smart home devices.

![CI](https://github.com/rvben/shelly-cli/actions/workflows/ci.yml/badge.svg) [![crates.io](https://img.shields.io/crates/v/shelly-cli.svg)](https://crates.io/crates/shelly-cli) [![PyPI](https://img.shields.io/pypi/v/shelly-cli.svg)](https://pypi.org/project/shelly-cli/) [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## Features

- Auto-discovery of Shelly devices on the local network (with subnet auto-detection)
- Unified Gen1 + Gen2/Gen3 support (transparent protocol handling)
- Multi-switch device support (e.g., Shelly 2.5 with dual relays)
- Interactive watch dashboard with live power, temperature, WiFi monitoring -- and switch control
- Energy consumption tracking (total kWh per device)
- Detailed device info view (model, firmware, uptime, WiFi, temperature)
- Device health checks (temperature, WiFi signal, firmware, uptime)
- Device authentication (`--password` flag or config file)
- Device groups with filter-based and name-based definitions
- Firmware check and update across all devices
- Config backup and restore with network-safe defaults
- Schedule and webhook inspection (Gen2/Gen3)
- Device renaming and configuration from the CLI
- Structured JSON output for scripting and AI agent integration
- Shell completions with dynamic device name suggestions (bash, zsh, fish)
- Fuzzy device name matching with "did you mean?" suggestions
- Color output with automatic detection

## Install

### uv (recommended)

```bash
uv tool install shelly-cli
```

### Homebrew (macOS/Linux)

```bash
brew install rvben/tap/shelly-cli
```

### pip

```bash
pip install shelly-cli
```

### Cargo

```bash
cargo install shelly-cli
```

### Pre-built binaries

Download from [GitHub Releases](https://github.com/rvben/shelly-cli/releases).

## Quick Start

```bash
# 1. Discover devices (auto-detects your subnet)
shelly discover

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
shelly energy -a                   # Total energy (kWh) per device
shelly info -n "Kitchen Light"     # Detailed device info
```

### Device management

```bash
shelly rename -n "old-name" "New Name"    # Rename device
shelly firmware check -a                   # Check for updates
shelly firmware update -a                  # Update all firmware
shelly reboot -n "Kitchen Light"          # Reboot device
```

### Configuration

```bash
shelly config get -n "Kitchen"                  # Get device config (JSON)
shelly config get -a                            # Get config for all devices
shelly config set -n "Kitchen" eco_mode true    # Set a config value
shelly config set -n "Kitchen" name "New Name"  # Rename via config
```

Supported config keys: `name`, `eco_mode`, `led_status_disable`.

### Backup and restore

```bash
shelly backup -n "Kitchen"         # Backup single device
shelly backup -a                   # Backup all devices to shelly-backups/
shelly backup -a -o ~/backups      # Custom output directory

shelly restore -n "Kitchen" shelly-backups/kitchen-2025-01-15.json
```

Restore skips network/WiFi/MQTT/cloud settings to avoid bricking devices.

### Schedules and webhooks

```bash
shelly schedule list -n "Kitchen"  # View device schedules (Gen2/Gen3)
shelly schedule list -a            # View all device schedules

shelly webhook list -n "Kitchen"   # View device webhooks
shelly webhook list -a             # View all device webhooks
```

### Groups

```bash
shelly group add lights "Kitchen" "Living Room" "Bedroom"
shelly group list
shelly group show lights
shelly -g lights off               # Turn off all lights
shelly -g lights status            # Status of all lights
shelly -g gen3 firmware check      # Check firmware for Gen3 devices
```

### Authentication

For devices with authentication enabled:

```bash
# Per-command
shelly --password "secret" status -a

# Or set in config file (~/.config/shelly-cli/config.toml)
# [auth]
# password = "secret"
```

### Shell completions

```bash
# Generate completions (includes dynamic device name suggestions)
shelly completions zsh > ~/.zfunc/_shelly    # zsh
shelly completions bash > /etc/bash_completion.d/shelly  # bash
shelly completions fish > ~/.config/fish/completions/shelly.fish  # fish

# After installing, tab-complete device names:
# shelly -n <TAB>  →  "Kitchen Light"  "Living Room"  "Bedroom Fan"
# shelly -g <TAB>  →  "lights"  "gen1"  "gen3"
```

## Agent Integration

Designed for scripting and AI agent use with structured, machine-readable output.

```bash
# Structured JSON output (auto-enabled when piped)
shelly status -a | jq '.data'

# Consistent envelope: {"ok": true, "data": ...} or {"ok": false, "error": {...}}
shelly -n "nonexistent" status
# {"ok": false, "error": {"code": "DEVICE_NOT_FOUND", "message": "..."}}

# Machine-readable schema with types, targeting docs, and error codes
shelly schema
```

Error codes: `DEVICE_NOT_FOUND`, `DEVICE_UNREACHABLE`, `AUTH_REQUIRED`, `NETWORK_ERROR`, `INVALID_INPUT`, `GROUP_NOT_FOUND`, `NO_CACHED_DEVICES`, `PARTIAL_FAILURE`.

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
| Gen1 | Shelly 1, 1PM, 2.5, Plug S, Dimmer | Supported (switch, power, firmware, config) |
| Gen2 | Shelly Plus 1, Plus 1PM, Plus 2PM | Supported (switch, power, firmware, config, schedules, webhooks) |
| Gen3 | Shelly Mini 1PM G3, Plus series G3 | Supported (switch, power, firmware, config, schedules, webhooks) |

## License

MIT License -- see [LICENSE](LICENSE) file.
