//! SY6970 power-management support for the T-Display-S3 Pro.

use alloc::string::String;
use drivers::sy6970::SY6970;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use esp_hal::Async;
use esp_hal::i2c::master::I2c;
use log::info;

/// I2C device type used by the PMU.
type PmuI2c = I2cDevice<'static, CriticalSectionRawMutex, I2c<'static, Async>>;

/// Errors returned by the unified charger API.
#[derive(Debug, Clone, Copy)]
pub struct PmuError;

/// SY6970 charger wrapped in the UI-facing API.
pub struct Charger(SY6970<PmuI2c>);

impl Charger {
    /// Returns an estimated battery percentage suitable for the UI.
    pub async fn get_battery_percentage(&mut self) -> Result<u8, PmuError> {
        let millivolts = self
            .0
            .get_battery_voltage_mv()
            .await
            .map_err(|_| PmuError)?;
        // The board charges a one-cell Li-ion battery to 4.352 V. This is
        // intentionally an estimate: voltage alone cannot precisely infer
        // state of charge under load.
        Ok(if millivolts <= 3_300 {
            0
        } else if millivolts >= 4_352 {
            100
        } else {
            ((u32::from(millivolts - 3_300) * 100) / (4_352 - 3_300)) as u8
        })
    }

    /// Returns whether the PMU is actively charging the battery.
    pub async fn is_charging(&mut self) -> Result<bool, PmuError> {
        self.0.is_charging().await.map_err(|_| PmuError)
    }

    /// Enables battery charging.
    pub async fn set_charge_enabled(&mut self) -> Result<(), PmuError> {
        self.0.set_charge_enabled(true).await.map_err(|_| PmuError)
    }

    /// Disables battery charging.
    pub async fn set_charge_disabled(&mut self) -> Result<(), PmuError> {
        self.0.set_charge_enabled(false).await.map_err(|_| PmuError)
    }

    /// Builds a human-readable PMU report for the detail view.
    pub async fn get_info(&mut self) -> Result<String, PmuError> {
        let voltage = self
            .0
            .get_battery_voltage_mv()
            .await
            .map_err(|_| PmuError)?;
        let status = self.0.get_charge_status().await.map_err(|_| PmuError)?;
        let enabled = self.0.is_charge_enabled().await.map_err(|_| PmuError)?;
        Ok(alloc::format!(
            "PMU: SY6970\nBattery voltage: {voltage} mV\nCharge status: {status:?}\nCharging enabled: {enabled}"
        ))
    }
}

/// Initializes the Pro's SY6970 PMU.
pub async fn initialize_pmu(i2c_device: PmuI2c) -> Charger {
    let mut pmu = SY6970::new(i2c_device);
    pmu.init().await.expect("Failed to initialize SY6970");
    info!("Initialized SY6970 PMU");
    Charger(pmu)
}
