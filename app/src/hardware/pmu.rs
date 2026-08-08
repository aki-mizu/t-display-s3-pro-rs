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
    /// Whether a detected battery is actively in its pre-charge or fast-charge
    /// phase.
    pub charging: bool,
    /// Voltage-derived state of charge, or `None` when the board has no usable
    /// battery input.
    pub battery_percentage: Option<u8>,
}

/// SY6970 charger wrapped in the UI-facing API.
pub struct Charger {
    pmu: SY6970<PmuI2c>,
    // The T-Display-S3 Pro's disconnected battery input can float after the
    // initial ADC reading. Keep the settled boot decision so periodic UI
    // polling cannot mistake a floating input for a charging battery.
    battery_present_at_boot: bool,
}

impl Charger {
    /// Reads all power facts needed by the UI without changing charger
    /// configuration. The ADC runs continuously after PMU initialization, so
    /// a periodic refresh does not need an additional settle delay.
    pub async fn get_power_status(&mut self) -> Result<PowerStatus, PmuError> {
        let battery_percentage = if self.battery_present_at_boot {
            let battery_voltage_mv = self
                .pmu
                .get_battery_voltage_mv()
                .await
                .map_err(|_| PmuError)?;
            Some(battery_percentage_from_voltage(battery_voltage_mv))
        } else {
            // Do not sample the floating disconnected battery input during
            // USB-only operation. It is not a valid state-of-charge signal.
            None
        };
        let system_status = self.pmu.get_system_status().await.map_err(|_| PmuError)?;

        Ok(PowerStatus {
            usb_present: system_status.usb_present,
            // With the pack physically disconnected, SY6970's phase bits can
            // still transiently indicate pre-charge. Do not surface that as
            // real charging when the settled boot check found no battery.
            charging: self.battery_present_at_boot && system_status.charge_status.is_charging(),
            battery_percentage,
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

    // On this board, the physical battery switch disconnects the pack but not
    // the SY6970. Disable charging before any ADC settling time or other board
    // startup load, so a USB-only boot cannot spend its first 120 ms trying to
    // charge a nonexistent battery and brown out the USB rail.
    pmu.set_charge_enabled(false)
        .await
        .expect("Failed to disable SY6970 charging during safe startup");
    pmu.init().await.expect("Failed to initialize SY6970");

    // Let the continuous ADC settle, then enable normal charging only after a
    // usable one-cell pack is confirmed. The SY6970 has no battery-present
    // status bit, so this follows LilyGO's documented voltage heuristic.
    Timer::after_millis(BATTERY_ADC_SETTLE_MS).await;
    let battery_voltage_mv = pmu
        .get_battery_voltage_mv()
        .await
        .expect("Failed to read SY6970 battery voltage");
    let battery_present_at_boot = battery_voltage_mv >= BATTERY_PRESENT_THRESHOLD_MV;
    if battery_present_at_boot {
        pmu.set_charge_enabled(true)
            .await
            .expect("Failed to enable SY6970 charging for detected battery");
        info!(
            "Battery detected ({battery_voltage_mv} mV); enabled SY6970 charging after safe startup"
        );
    } else {
        info!(
            "No usable battery detected ({battery_voltage_mv} mV); kept SY6970 charging disabled for stable USB power"
        );
    }
    info!("Initialized SY6970 PMU");
    Charger {
        pmu,
        battery_present_at_boot,
    }
}
