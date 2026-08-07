#![no_std]

//! Board-independent presentation layer for the Bitcoin UI demo.
//!
//! This crate owns the generated Slint window, UI navigation, and the small
//! API that firmware uses to publish board state. It deliberately has no
//! ESP32, PMU, wallet, key, or networking dependencies.

extern crate alloc;

use slint::ComponentHandle;

mod generated {
    slint::include_modules!();
}

use generated::AppWindow;

/// A failure returned while creating or showing the Slint UI.
#[derive(Debug)]
pub struct UiError;

/// Battery facts supplied by board firmware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryState {
    pub percentage: u8,
    pub charging: bool,
}

/// Board facts that can be presented in the Settings screen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceStatus {
    CameraDetected { sccb_address: u8 },
    CameraNotDetected,
    Battery(BatteryState),
    BatteryUnavailable,
    ChargerStateUnavailable { percentage: u8 },
}

/// The public, board-independent interface to the Bitcoin demo window.
///
/// Screen navigation, demo account data, and user-facing strings remain in
/// Slint or this wrapper. Firmware supplies typed board facts and subscribes
/// only to the refresh request.
pub struct WalletUi {
    window: AppWindow,
}

impl WalletUi {
    /// Creates the compact 480 by 222 Bitcoin demo window.
    pub fn new() -> Result<Self, UiError> {
        Ok(Self {
            window: AppWindow::new().map_err(|_| UiError)?,
        })
    }

    /// Makes the window visible through the configured Slint platform.
    pub fn show(&self) -> Result<(), UiError> {
        self.window.show().map_err(|_| UiError)
    }

    /// Presents typed board state without exposing Slint properties to the
    /// firmware crate.
    pub fn set_device_status(&self, status: DeviceStatus) {
        match status {
            DeviceStatus::CameraDetected { sccb_address } => self.window.set_device_status(
                alloc::format!(
                    "Camera Shield detected at SCCB address 0x{sccb_address:02X}. Battery status can be refreshed below."
                )
                .into(),
            ),
            DeviceStatus::CameraNotDetected => self.window.set_device_status(
                "No Camera Shield sensor detected. Battery status can be refreshed below."
                    .into(),
            ),
            DeviceStatus::Battery(state) => self.set_battery_state(state),
            DeviceStatus::BatteryUnavailable => self.window.set_device_status(
                "Battery status unavailable. Check the PMU connection and try again."
                    .into(),
            ),
            DeviceStatus::ChargerStateUnavailable { percentage } => {
                self.window
                    .set_battery_percentage(i32::from(percentage.min(100)));
                self.window.set_device_status(
                    "Battery level updated, but charger state is unavailable."
                        .into(),
                );
            }
        }
    }

    /// Registers the one user action that needs a hardware refresh.
    pub fn on_refresh_device_status(&self, handler: impl Fn() + 'static) {
        self.window.on_request_device_status(handler);
    }

    fn set_battery_state(&self, state: BatteryState) {
        let percentage = state.percentage.min(100);
        self.window.set_battery_percentage(i32::from(percentage));
        self.window.set_charging(state.charging);
        self.window.set_device_status(
            alloc::format!(
                "Battery: {percentage}% / Charger: {}",
                if state.charging {
                    "charging"
                } else {
                    "not charging"
                }
            )
            .into(),
        );
    }
}
