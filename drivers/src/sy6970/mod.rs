//! Async driver for the SY6970 single-cell battery charger and power-path IC.
//!
//! The LilyGO T-Display-S3 Pro exposes the SY6970 on I2C address
//! `0x6A`. The driver deliberately preserves the board's factory-programmed
//! charging limits during initialization.

use embedded_hal::i2c::Error;

/// SY6970 I2C slave address.
pub const SY6970_ADDRESS: u8 = 0x6A;

/// The charger phase reported in the SY6970 system-status register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ChargeStatus {
    /// No charging operation is active.
    NotCharging,
    /// The battery is being charged in the pre-charge phase.
    PreCharge,
    /// The battery is being charged in the fast-charge phase.
    FastCharge,
    /// Charging has completed.
    Done,
}

impl ChargeStatus {
    /// Returns whether the charger is actively charging the battery.
    pub const fn is_charging(self) -> bool {
        matches!(self, Self::PreCharge | Self::FastCharge)
    }
}

/// Input and charge facts reported by the SY6970 system-status register.
///
/// `usb_present` describes a USB input source rather than an OTG output. The
/// separate `input_power_good` bit indicates that the detected input is
/// currently usable by the PMU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SystemStatus {
    /// A USB power source is connected to the board.
    pub usb_present: bool,
    /// The connected input source is currently power-good.
    pub input_power_good: bool,
    /// The current battery charge phase.
    pub charge_status: ChargeStatus,
}

/// Errors produced by the SY6970 driver.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Sy6970Error {
    /// An I2C transaction failed.
    I2cError,
}

impl<E> From<E> for Sy6970Error
where
    E: Error,
{
    fn from(_: E) -> Self {
        Self::I2cError
    }
}

/// SY6970 charger and power-path controller.
#[derive(Debug)]
pub struct SY6970<I2C> {
    pub(crate) i2c: I2C,
}

impl<I2C> SY6970<I2C> {
    /// Creates an SY6970 driver using the supplied I2C bus.
    pub fn new(i2c: I2C) -> Self {
        Self { i2c }
    }
}

#[cfg(feature = "async")]
pub mod asynch;

#[cfg(test)]
mod tests {
    use super::ChargeStatus;

    #[test]
    fn charge_status_reports_only_active_charge_phases() {
        assert!(!ChargeStatus::NotCharging.is_charging());
        assert!(ChargeStatus::PreCharge.is_charging());
        assert!(ChargeStatus::FastCharge.is_charging());
        assert!(!ChargeStatus::Done.is_charging());
    }
}
