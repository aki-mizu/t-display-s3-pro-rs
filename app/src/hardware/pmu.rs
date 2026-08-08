//! SY6970 power-management support for the T-Display-S3 Pro.

use drivers::sy6970::SY6970;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Timer;
use esp_hal::Async;
use esp_hal::i2c::master::I2c;
use log::info;

/// I2C device type used by the PMU.
type PmuI2c = I2cDevice<'static, CriticalSectionRawMutex, I2c<'static, Async>>;

// LilyGO's board example uses this threshold to distinguish its disconnected
// battery input from a one-cell pack. The SY6970 lacks a dedicated
// battery-present status bit, so this is necessarily a board-specific
// best-effort check.
const BATTERY_PRESENT_THRESHOLD_MV: u16 = 3_000;
const BATTERY_ADC_SETTLE_MS: u64 = 120;

/// Errors returned by the unified charger API.
#[derive(Debug, Clone, Copy)]
pub struct PmuError;

/// Power facts suitable for the compact on-screen status indicator.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PowerStatus {
    /// Whether the PMU detects an external USB power input.
    pub usb_present: bool,
    /// Whether the charger is actively in its pre-charge or fast-charge phase.
    pub charging: bool,
    /// Voltage-derived state of charge, or `None` when the board has no usable
    /// battery input.
    pub battery_percentage: Option<u8>,
}

/// SY6970 charger wrapped in the UI-facing API.
pub struct Charger(SY6970<PmuI2c>);

impl Charger {
    /// Reads all power facts needed by the UI without changing charger
    /// configuration. The ADC runs continuously after PMU initialization, so
    /// a periodic refresh does not need an additional settle delay.
    pub async fn get_power_status(&mut self) -> Result<PowerStatus, PmuError> {
        let battery_voltage_mv = self
            .0
            .get_battery_voltage_mv()
            .await
            .map_err(|_| PmuError)?;
        let system_status = self.0.get_system_status().await.map_err(|_| PmuError)?;

        Ok(PowerStatus {
            usb_present: system_status.usb_present,
            charging: system_status.charge_status.is_charging(),
            // Match the same best-effort presence threshold used by the
            // USB-only startup safeguard. This avoids presenting a
            // disconnected battery as a misleading 0% charge level.
            battery_percentage: (battery_voltage_mv >= BATTERY_PRESENT_THRESHOLD_MV)
                .then(|| battery_percentage_from_voltage(battery_voltage_mv)),
        })
    }
}

fn battery_percentage_from_voltage(millivolts: u16) -> u8 {
    // The board charges a one-cell Li-ion battery to 4.352 V. This is
    // intentionally an estimate: voltage alone cannot precisely infer
    // state of charge under load.
    if millivolts <= 3_300 {
        0
    } else if millivolts >= 4_352 {
        100
    } else {
        ((u32::from(millivolts - 3_300) * 100) / (4_352 - 3_300)) as u8
    }
}

/// Initializes the Pro's SY6970 PMU.
pub async fn initialize_pmu(i2c_device: PmuI2c) -> Charger {
    let mut pmu = SY6970::new(i2c_device);
    pmu.init().await.expect("Failed to initialize SY6970");

    // The T-Display-S3 Pro's SY6970 limits the system input current if
    // charging remains enabled with no battery connected. Let its ADC settle,
    // then mirror LilyGO's conditional workaround: disable charging in that
    // USB-only case, while preserving normal charging for a connected pack.
    Timer::after_millis(BATTERY_ADC_SETTLE_MS).await;
    let battery_voltage_mv = pmu
        .get_battery_voltage_mv()
        .await
        .expect("Failed to read SY6970 battery voltage");
    if battery_voltage_mv < BATTERY_PRESENT_THRESHOLD_MV {
        pmu.set_charge_enabled(false)
            .await
            .expect("Failed to disable SY6970 charging for USB-only power");
        info!(
            "No usable battery detected ({battery_voltage_mv} mV); disabled SY6970 charging for stable USB power"
        );
    } else {
        // Retain the board's factory-programmed charging limits and state.
        // This USB-only safeguard must not override normal battery charging.
        info!("Battery detected ({battery_voltage_mv} mV); retained SY6970 charging configuration");
    }
    info!("Initialized SY6970 PMU");
    Charger(pmu)
}
