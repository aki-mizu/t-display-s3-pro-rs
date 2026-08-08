//! Async driver for the Hynitron CST226SE capacitive touch controller.
//!
//! The CST226SE is used on the LilyGo T-Display-S3-Pro. It reports up to five
//! touches in a 28-byte report on I2C address `0x5A`. This driver currently
//! exposes the first touch point only.

use embedded_hal::i2c::Error;

/// CST226SE I2C slave address.
pub const CST226SE_ADDRESS: u8 = 0x5A;

/// Size of a CST226SE touch report.
pub const RAW_TOUCH_REPORT_LEN: usize = 28;

/// Maximum number of touches represented by one report.
pub const MAX_TOUCH_POINTS: u8 = 5;

/// CST226SE point-state value meaning an active finger contact.
///
/// The controller also sends a point record for a lift; its state is any
/// value other than this one and must not be interpreted as another press.
pub const TOUCH_STATUS_PRESSED: u8 = 0x06;

const REPORT_VALID_MARKER: u8 = 0xAB;
const HOME_BUTTON_MARKER: u8 = 0x80;
const FIRMWARE_CHECKCODE_PREFIX: u32 = 0xCACA_0000;

/// The first touch point in a CST226SE report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TouchPoint {
    /// Number of active touches reported by the controller.
    pub points: u8,
    /// Controller-assigned identifier for this contact.
    pub id: u8,
    /// Controller-specific contact state.
    pub status: u8,
    /// Horizontal coordinate.
    pub x: u16,
    /// Vertical coordinate.
    pub y: u16,
    /// Contact pressure reported by the controller.
    pub pressure: u8,
}

/// Errors produced by the CST226SE driver.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TouchSensorError {
    /// An I2C transaction failed.
    I2cError,
    /// Reading the interrupt pin or driving the reset pin failed.
    PinError,
    /// The CST226SE firmware checkcode did not match the expected value.
    InvalidDevice,
}

impl<E> From<E> for TouchSensorError
where
    E: Error,
{
    fn from(_: E) -> Self {
        Self::I2cError
    }
}

/// CST226SE touch controller.
///
/// `PIN` is the active-low interrupt pin, `RST` is the optional reset pin,
/// and `DELAY` supplies reset timing.
#[derive(Debug)]
pub struct CST226SE<I2C, PIN, RST, DELAY> {
    pub(crate) i2c: I2C,
    pub(crate) touch_int: PIN,
    pub(crate) rst_pin: Option<RST>,
    pub(crate) delay: DELAY,
}

impl<I2C, PIN, RST, DELAY> CST226SE<I2C, PIN, RST, DELAY> {
    /// Creates a CST226SE driver instance.
    pub fn new(i2c: I2C, touch_int: PIN, rst_pin: Option<RST>, delay: DELAY) -> Self {
        Self {
            i2c,
            touch_int,
            rst_pin,
            delay,
        }
    }
}

pub(crate) fn parse_first_point(report: &[u8; RAW_TOUCH_REPORT_LEN]) -> Option<TouchPoint> {
    if report[6] != REPORT_VALID_MARKER
        || report[0] == REPORT_VALID_MARKER
        || report[5] == HOME_BUTTON_MARKER
    {
        return None;
    }

    let points = report[5] & 0x7F;
    if points == 0 || points > MAX_TOUCH_POINTS {
        return None;
    }

    Some(TouchPoint {
        points,
        id: report[0] >> 4,
        status: report[0] & 0x0F,
        x: (u16::from(report[1]) << 4) | u16::from(report[3] >> 4),
        y: (u16::from(report[2]) << 4) | u16::from(report[3] & 0x0F),
        pressure: report[4],
    })
}

pub(crate) fn has_valid_firmware_checkcode(checkcode: [u8; 4]) -> bool {
    (u32::from_le_bytes(checkcode) & 0xFFFF_0000) == FIRMWARE_CHECKCODE_PREFIX
}

/// Returns whether a point record represents an active finger contact.
pub const fn is_touch_pressed(status: u8) -> bool {
    status == TOUCH_STATUS_PRESSED
}

/// The vendor driver acknowledges reports with an invalid (including zero)
/// point count so the controller can publish the next report.
pub(crate) fn should_acknowledge_empty_report(report: &[u8; RAW_TOUCH_REPORT_LEN]) -> bool {
    if report[6] != REPORT_VALID_MARKER
        || report[0] == REPORT_VALID_MARKER
        || report[5] == HOME_BUTTON_MARKER
    {
        return false;
    }

    let points = report[5] & 0x7F;
    points == 0 || points > MAX_TOUCH_POINTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_first_touch_point() {
        let mut report = [0u8; RAW_TOUCH_REPORT_LEN];
        report[0] = 0x23;
        report[1] = 0x12;
        report[2] = 0x34;
        report[3] = 0x56;
        report[4] = 0x78;
        report[5] = 0x01;
        report[6] = REPORT_VALID_MARKER;

        assert_eq!(
            parse_first_point(&report),
            Some(TouchPoint {
                points: 1,
                id: 2,
                status: 3,
                x: 0x125,
                y: 0x346,
                pressure: 0x78,
            })
        );
    }

    #[test]
    fn ignores_home_button_reports() {
        let mut report = [0u8; RAW_TOUCH_REPORT_LEN];
        report[5] = HOME_BUTTON_MARKER;
        report[6] = REPORT_VALID_MARKER;

        assert_eq!(parse_first_point(&report), None);
    }

    #[test]
    fn parses_a_zero_id_lift_record_for_the_input_layer() {
        let mut report = [0u8; RAW_TOUCH_REPORT_LEN];
        report[0] = 0;
        report[1] = 0x12;
        report[2] = 0x34;
        report[3] = 0x56;
        report[5] = 0x01;
        report[6] = REPORT_VALID_MARKER;

        assert_eq!(
            parse_first_point(&report),
            Some(TouchPoint {
                points: 1,
                id: 0,
                status: 0,
                x: 0x125,
                y: 0x346,
                pressure: 0,
            })
        );
        assert!(!should_acknowledge_empty_report(&report));
    }

    #[test]
    fn recognizes_the_controller_firmware_checkcode() {
        assert!(has_valid_firmware_checkcode([0x12, 0x34, 0xCA, 0xCA]));
        assert!(!has_valid_firmware_checkcode([0x12, 0x34, 0x00, 0xCA]));
    }

    #[test]
    fn distinguishes_pressed_and_lifted_point_records() {
        assert!(is_touch_pressed(TOUCH_STATUS_PRESSED));
        assert!(!is_touch_pressed(0));
        assert!(!is_touch_pressed(0x07));
    }
}

#[cfg(feature = "async")]
pub mod asynch;
