//! SY6970 power-management support for the T-Display-S3 Pro.

use drivers::sy6970::{ChargeStatus, SY6970};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Timer;
use esp_hal::Async;
use esp_hal::i2c::master::I2c;
use log::info;

/// I2C device type used by the PMU.
type PmuI2c = I2cDevice<'static, CriticalSectionRawMutex, I2c<'static, Async>>;

// The SY6970 lacks a battery-present bit. LilyGO's board example uses this
// threshold to distinguish its disconnected battery input from a one-cell
// pack. It is necessarily a board-specific, boot-time best-effort check.
const BATTERY_PRESENT_THRESHOLD_MV: u16 = 3_000;
// Match Jade's T-Display S3 Pro Camera status bar: once the PMU is no longer
// in an active charge phase, a one-cell pack above this voltage is presented
// as a static full battery. The PMU can report `NotCharging` rather than the
// distinct `Done` state after it terminates a charge cycle.
const JADE_FULL_BATTERY_THRESHOLD_MV: u16 = 4_000;
const BATTERY_ADC_SETTLE_MS: u64 = 120;

/// Errors returned by the unified charger API.
#[derive(Debug, Clone, Copy)]
pub struct PmuError;

/// Power facts suitable for the compact on-screen status indicator.
///
/// USB presence comes from SY6970 `REG11.BUS_GD`, while the charge phase comes
/// from `REG0B.CHRG_STAT`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PowerStatus {
    /// Whether SY6970 reports a physical external USB input.
    pub usb_present: bool,
    /// Whether the PMU reports an active pre-charge or fast-charge phase.
    pub charging: bool,
    /// Whether the compact indicator should present a static full battery.
    pub charge_complete: bool,
    /// Voltage-derived state of charge, or `None` when the board has no usable
    /// battery input.
    pub battery_percentage: Option<u8>,
}

/// SY6970 charger wrapped in the UI-facing API.
pub struct Charger {
    pmu: SY6970<PmuI2c>,
    // With the physical battery switch off, the BAT ADC can float once USB is
    // present. Keep the settled boot decision so later floating readings
    // cannot make the UI report a fictional pack or trigger charge animation.
    battery_present_at_boot: bool,
}

impl Charger {
    /// Reads board power facts without writing PMU registers after
    /// initialization. USB-only operation intentionally avoids the floating
    /// BAT ADC and reads only the PMU's USB/charge status register.
    pub async fn get_power_status(&mut self) -> Result<PowerStatus, PmuError> {
        let system_status = self.pmu.get_system_status().await.map_err(|_| PmuError)?;
        let battery_voltage_mv = if self.battery_present_at_boot {
            let battery_voltage_mv = self
                .pmu
                .get_battery_voltage_mv()
                .await
                .map_err(|_| PmuError)?;
            Some(battery_voltage_mv)
        } else {
            None
        };
        let charging = self.battery_present_at_boot && system_status.charge_status.is_charging();
        let charge_complete = self.battery_present_at_boot
            && (system_status.charge_status == ChargeStatus::Done
                || (!charging
                    && battery_voltage_mv
                        .is_some_and(|millivolts| millivolts > JADE_FULL_BATTERY_THRESHOLD_MV)));

        Ok(PowerStatus {
            usb_present: system_status.usb_present,
            charging,
            charge_complete,
            battery_percentage: battery_voltage_mv.map(battery_percentage_from_voltage),
        })
    }
}

fn battery_percentage_from_voltage(millivolts: u16) -> u8 {
    // Match Jade's T-Display S3 Pro Camera bands exactly. These are display
    // levels, not a precise state-of-charge calculation: voltage varies with
    // battery load and temperature.
    if millivolts <= 3_200 {
        0
    } else if millivolts <= 3_400 {
        20
    } else if millivolts <= 3_600 {
        40
    } else if millivolts <= 3_800 {
        60
    } else if millivolts <= 4_000 {
        80
    } else {
        100
    }
}

#[cfg(test)]
mod tests {
    use super::battery_percentage_from_voltage;

    #[test]
    fn uses_jades_five_battery_voltage_bands() {
        assert_eq!(battery_percentage_from_voltage(3_200), 0);
        assert_eq!(battery_percentage_from_voltage(3_201), 20);
        assert_eq!(battery_percentage_from_voltage(3_400), 20);
        assert_eq!(battery_percentage_from_voltage(3_401), 40);
        assert_eq!(battery_percentage_from_voltage(3_600), 40);
        assert_eq!(battery_percentage_from_voltage(3_601), 60);
        assert_eq!(battery_percentage_from_voltage(3_800), 60);
        assert_eq!(battery_percentage_from_voltage(3_801), 80);
        assert_eq!(battery_percentage_from_voltage(4_000), 80);
        assert_eq!(battery_percentage_from_voltage(4_001), 100);
    }
}

/// Initializes the Pro's SY6970 PMU.
pub async fn initialize_pmu(i2c_device: PmuI2c) -> Charger {
    let mut pmu = SY6970::new(i2c_device);

    // LilyGO documents that the Pro can shut down when USB powers the board
    // without a connected battery unless SY6970 charging is disabled. Do this
    // before display/camera startup, then restore normal charging only when
    // the settled BAT ADC reports a usable pack.
    pmu.set_charge_enabled(false)
        .await
        .expect("Failed to disable SY6970 charging during USB-only startup");
    pmu.init().await.expect("Failed to initialize SY6970");

    Timer::after_millis(BATTERY_ADC_SETTLE_MS).await;
    let battery_voltage_mv = pmu
        .get_battery_voltage_mv()
        .await
        .expect("Failed to read SY6970 battery voltage");
    let battery_present_at_boot = battery_voltage_mv >= BATTERY_PRESENT_THRESHOLD_MV;
    if battery_present_at_boot {
        pmu.set_charge_enabled(true)
            .await
            .expect("Failed to enable SY6970 charging for connected battery");
        info!("Battery detected ({battery_voltage_mv} mV); charging enabled");
    } else {
        info!("No battery detected ({battery_voltage_mv} mV); charging remains disabled");
    }

    Charger {
        pmu,
        battery_present_at_boot,
    }
}
