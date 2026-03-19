use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "shelly-cli", about = "CLI for managing Shelly devices", version)]
pub struct Cli {
    /// Target device by IP address
    #[arg(long, global = true)]
    pub host: Option<String>,

    /// Target device by name (uses cached device list)
    #[arg(long, global = true)]
    pub name: Option<String>,

    /// Output as JSON (auto-enabled when stdout is not a terminal)
    #[arg(long, global = true)]
    pub json: bool,

    /// Suppress non-data output
    #[arg(long, global = true)]
    pub quiet: bool,

    /// HTTP timeout in milliseconds
    #[arg(long, global = true, default_value = "3000")]
    pub timeout: u64,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Scan network for Shelly devices
    Discover {
        /// Subnet to scan (CIDR notation, e.g. 10.10.20.0/24)
        #[arg(long)]
        subnet: Option<String>,
    },

    /// List known/cached devices
    Devices {
        /// Re-scan network before listing
        #[arg(long)]
        refresh: bool,
    },

    /// Get device status
    Status {
        /// Query all known devices
        #[arg(long)]
        all: bool,

        /// Switch/relay ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },

    /// Control switch/relay outputs
    Switch {
        #[command(subcommand)]
        action: SwitchAction,
    },

    /// Energy and power monitoring
    Power {
        /// Query all known devices
        #[arg(long)]
        all: bool,

        /// Meter ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },

    /// Check or update firmware
    Firmware {
        #[command(subcommand)]
        action: FirmwareAction,
    },

    /// Get or set device configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Reboot a device
    Reboot,

    /// Dump all commands as JSON for agent introspection
    Schema,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Clone)]
pub enum SwitchAction {
    /// Get switch status
    Status {
        /// Switch ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
    /// Turn switch on
    On {
        /// Switch ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
    /// Turn switch off
    Off {
        /// Switch ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
    /// Toggle switch
    Toggle {
        /// Switch ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
}

#[derive(Subcommand, Clone)]
pub enum FirmwareAction {
    /// Check for available updates
    Check {
        /// Check all known devices
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand, Clone)]
pub enum ConfigAction {
    /// Get device configuration
    Get,
}
