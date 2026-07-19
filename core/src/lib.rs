pub mod api;
pub mod model;

pub use api::discovery::{enrich_gen1_name, scan_subnet};
pub use api::gen1::Gen1Device;
pub use api::gen2::Gen2Device;
pub use api::{FirmwareInfo, ShellyDevice, SwitchResult, create_device, probe_device};
pub use model::status::{InputStatus, WifiStatus};
pub use model::{
    DeviceGeneration, DeviceInfo, DeviceStatus, LightComponent, LightKind, LightParams,
    LightStatus, PowerReading, SwitchStatus,
};
