pub mod api;
pub mod error;
pub mod model;
pub mod switchkit_impl;

pub use api::discovery::{enrich_gen1_name, scan_subnet};
pub use api::gen1::Gen1Device;
pub use api::gen2::Gen2Device;
pub use api::{
    FirmwareInfo, ShellyDevice, SwitchResult, create_device, create_device_with_host, probe_device,
    probe_target,
};
pub use error::{Error, Result};
pub use model::status::{InputStatus, WifiStatus};
pub use model::{
    DeviceGeneration, DeviceInfo, DeviceStatus, LightComponent, LightKind, LightParams,
    LightStatus, PowerReading, SwitchStatus,
};
pub use switchkit_impl::ShellyClient;
