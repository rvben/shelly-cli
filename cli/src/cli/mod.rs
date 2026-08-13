use clap::{Args, Parser, Subcommand};

/// Pagination and field-selection flags shared by list commands.
#[derive(Args, Clone)]
pub struct ListArgs {
    /// Maximum number of items to return
    #[arg(long, default_value = "100")]
    pub limit: usize,
    /// Number of items to skip before returning results
    #[arg(long, default_value = "0")]
    pub offset: usize,
    /// Comma-separated list of field names to include in each item
    #[arg(long)]
    pub fields: Option<String>,
}

#[derive(Parser)]
#[command(
    about = "CLI for managing Shelly devices. Run 'shelly schema' for machine-readable introspection.",
    version,
    after_long_help = "\
Examples:
  shelly discover --subnet 192.168.1.0/24
  shelly on \"Kitchen Light\"
  shelly on \"Office Strip\" --id 1
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

    /// Output format: auto detects TTY (default), json, or text
    #[arg(long, short = 'o', global = true, default_value = "auto",
          value_parser = ["auto", "text", "json"])]
    pub output: String,

    /// Force JSON output (kept for backwards compatibility; prefer --output json)
    #[arg(long, short = 'j', global = true, hide = true)]
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
        #[command(flatten)]
        list: ListArgs,
    },

    /// Get device status
    Status {
        /// Query all known devices
        #[arg(long, short = 'a')]
        all: bool,
        #[command(flatten)]
        list: ListArgs,
    },

    /// Control switch/relay outputs
    Switch {
        #[command(subcommand)]
        action: SwitchAction,
    },

    /// Control RGB / RGBW / CCT / dimmable light outputs (Gen2/Gen3)
    Light {
        #[command(subcommand)]
        action: LightAction,
    },

    /// Turn device(s) on
    On {
        /// Device name (positional for convenience)
        device: Option<String>,
        /// Switch/plug ID for multi-channel devices (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },

    /// Turn device(s) off
    Off {
        /// Device name (positional for convenience)
        device: Option<String>,
        /// Switch/plug ID for multi-channel devices (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },

    /// Toggle device(s)
    Toggle {
        /// Device name (positional for convenience)
        device: Option<String>,
        /// Switch/plug ID for multi-channel devices (default: 0)
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

    /// Manage device groups
    Group {
        #[command(subcommand)]
        action: GroupAction,
    },

    /// View device schedules
    Schedule {
        #[command(subcommand)]
        action: ScheduleAction,
    },

    /// View device webhooks
    Webhook {
        #[command(subcommand)]
        action: WebhookAction,
    },

    /// Backup device configuration to a JSON file
    Backup {
        /// Backup all known devices
        #[arg(long, short = 'a')]
        all: bool,
        /// Output directory (default: current directory)
        #[arg(long)]
        dir: Option<String>,
    },

    /// Restore device configuration from a backup file
    Restore {
        /// Path to the backup JSON file
        file: String,
        /// Skip confirmation prompt (required when stdin is not a terminal)
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Rename a device
    Rename {
        /// New name for the device
        new_name: String,
        /// Skip confirmation prompt (required when stdin is not a terminal)
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Reboot a device
    Reboot {
        /// Skip confirmation prompt (required when stdin is not a terminal)
        #[arg(long, short = 'y')]
        yes: bool,
    },

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

    /// Output a machine-readable JSON description of all commands, arguments, and error kinds
    Schema,

    /// Describe supported device generations without network access
    Capabilities,

    /// Generate shell completions (with dynamic device name completion)
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },

    /// Output cached device names for shell completion
    #[command(name = "_complete-device-names", hide = true)]
    CompleteDeviceNames,

    /// Output group names for shell completion
    #[command(name = "_complete-group-names", hide = true)]
    CompleteGroupNames,
}

#[derive(Subcommand, Clone)]
pub enum SwitchAction {
    /// Get switch status
    Status {
        /// Switch/plug ID for multi-channel devices (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
    /// Turn switch on
    On {
        /// Switch/plug ID for multi-channel devices (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
    /// Turn switch off
    Off {
        /// Switch/plug ID for multi-channel devices (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
    /// Toggle switch
    Toggle {
        /// Switch/plug ID for multi-channel devices (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
}

/// Color/brightness attributes shared by `light on` and `light set`.
#[derive(Args, Clone)]
pub struct LightSetArgs {
    /// Light component ID (default: 0)
    #[arg(long, default_value = "0")]
    pub id: u8,
    /// Color as hex (#00ff88) or name (red, green, warm, ...)
    #[arg(long, conflicts_with = "rgb")]
    pub color: Option<String>,
    /// Color as comma-separated r,g,b (each 0-255), e.g. 0,255,136
    #[arg(long)]
    pub rgb: Option<String>,
    /// Brightness 1-100 (RGB/RGBW) or 0-100 (CCT/dimmable)
    #[arg(long)]
    pub brightness: Option<u8>,
    /// White channel 0-255 (RGBW only)
    #[arg(long)]
    pub white: Option<u8>,
    /// Color temperature in Kelvin (CCT only)
    #[arg(long)]
    pub temp: Option<u32>,
}

#[derive(Subcommand, Clone)]
pub enum LightAction {
    /// Show light status (on/off, color, brightness)
    Status {
        /// Light component ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
    /// Turn light on, optionally setting color/brightness/white/temp
    On {
        #[command(flatten)]
        args: LightSetArgs,
    },
    /// Turn light off
    Off {
        /// Light component ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
    /// Toggle light on/off
    Toggle {
        /// Light component ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
    /// Change attributes without changing power state
    Set {
        #[command(flatten)]
        args: LightSetArgs,
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
        /// Skip confirmation prompt (required when stdin is not a terminal)
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand, Clone)]
pub enum ConfigAction {
    /// Get device configuration
    Get {
        /// Get config for all devices
        #[arg(long, short = 'a')]
        all: bool,
    },
    /// Set a device configuration value (e.g. eco_mode true)
    Set {
        /// Configuration key (e.g. eco_mode, name, led_status_disable)
        key: String,
        /// Value to set
        value: String,
    },
}

#[derive(Subcommand, Clone)]
pub enum ScheduleAction {
    /// List device schedules
    List {
        /// List schedules for all devices
        #[arg(long, short = 'a')]
        all: bool,
        #[command(flatten)]
        list: ListArgs,
    },
}

#[derive(Subcommand, Clone)]
pub enum WebhookAction {
    /// List device webhooks
    List {
        /// List webhooks for all devices
        #[arg(long, short = 'a')]
        all: bool,
        #[command(flatten)]
        list: ListArgs,
    },
}

#[derive(Subcommand, Clone)]
pub enum GroupAction {
    /// List all defined groups
    List {
        #[command(flatten)]
        list: ListArgs,
    },
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
        /// Skip confirmation prompt (required when stdin is not a terminal)
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Show devices in a group
    Show {
        /// Group name
        name: String,
    },
}
