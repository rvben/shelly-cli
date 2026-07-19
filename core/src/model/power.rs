use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerReading {
    pub id: u8,
    pub power_watts: f64,
    pub voltage: Option<f64>,
    pub current: Option<f64>,
    pub total_energy_wh: f64,
}
