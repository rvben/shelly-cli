use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    about = "CLI for managing Shelly devices",
    version,
    after_long_help = "\
Examples:
  shelly discover --subnet 192.168.1.0/24
  shelly on \"Kitchen Light\"
  shelly status -n \"Living Room\"
  shelly power -a
  shelly energy -a
  shelly health
  shelly watch
  shelly -g lights off"
)]
pub struct Cli {
    /// Target device by IP address
    #[arg(long, global = true)]
    pub host: Option<String>,

    /// Target device by name (uses cached device list)
    #[arg(long, short = 'n', global = true)]
    pub name: Option<String>,

    /// Target a device group (defined in groups.toml)
    #[arg(long, short = 'g', global = true)]
    pub group: Option<String>,

    /// Output as JSON (auto-enabled when stdout is not a terminal)
    #[arg(long, short = 'j', global = true)]
    pub json: bool,

    /// Suppress non-data output
    #[arg(long, short = 'q', global = true)]
    pub quiet: bool,

    /// Device password for authentication
    #[arg(long, short = 'p', global = true)]
    pub password: Option<String>,

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
        #[arg(long, short = 'a')]
        all: bool,
    },

    /// Control switch/relay outputs
    Switch {
        #[command(subcommand)]
        action: SwitchAction,
    },

    /// Turn device(s) on
    On {
        /// Device name (positional for convenience)
        device: Option<String>,
        /// Switch ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },

    /// Turn device(s) off
    Off {
        /// Device name (positional for convenience)
        device: Option<String>,
        /// Switch ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },

    /// Toggle device(s)
    Toggle {
        /// Device name (positional for convenience)
        device: Option<String>,
        /// Switch ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },

    /// Energy and power monitoring
    Power {
        /// Query all known devices
        #[arg(long, short = 'a')]
        all: bool,

        /// Meter ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },

    /// Show total energy consumption (kWh) across devices
    Energy {
        /// Query all known devices
        #[arg(long, short = 'a')]
        all: bool,
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

    /// Rename a device
    Rename {
        /// New name for the device
        new_name: String,
    },

    /// Reboot a device
    Reboot,

    /// Live-updating dashboard of all devices
    Watch {
        /// Refresh interval in seconds
        #[arg(long, default_value = "2")]
        interval: u64,
    },

    /// Show detailed information about a device
    Info,

    /// Check device health (temperature, WiFi, firmware, online status)
    Health,

    /// Manage device groups
    Group {
        #[command(subcommand)]
        action: GroupAction,
    },

    /// Dump all commands as JSON for agent introspection
    #[command(hide = true)]
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
        #[arg(long, short = 'a')]
        all: bool,
    },
    /// Update firmware to latest stable version
    Update {
        /// Update all known devices
        #[arg(long, short = 'a')]
        all: bool,
    },
}

#[derive(Subcommand, Clone)]
pub enum ConfigAction {
    /// Get device configuration
    Get,
}

#[derive(Subcommand, Clone)]
pub enum GroupAction {
    /// List all defined groups
    List,
    /// Add a new group
    Add {
        /// Group name
        name: String,
        /// Device names to include
        #[arg(required = true)]
        devices: Vec<String>,
    },
    /// Remove a group
    Remove {
        /// Group name to remove
        name: String,
    },
    /// Show devices in a group
    Show {
        /// Group name
        name: String,
    },
}
