use bitcoin_ui::{BatteryState, DeviceStatus, MNEMONIC_PREFIX_WORD_COUNT, WalletUi};
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Ticker};
use esp_hal::rng::Trng;
use log::{error, info};

use crate::Charger;

pub struct Controller<'a> {
    ui: &'a WalletUi,
    pmu: Charger,
    last_power_state: Option<PresentedPowerState>,
}

#[derive(Debug, Clone)]
enum Action {
    RefreshDeviceStatus,
    GenerateRandomWords,
}

/// The visible power facts painted into the UI. Retaining this separately from
/// the full PMU snapshot lets the controller poll in the background without
/// repainting when only a non-visible charge phase changes.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum PresentedPowerState {
    Available(PowerIndicator),
    Unavailable,
}

/// The compact indicator only presents USB presence and a quantized battery
/// fill level. Charging-state samples remain available to the firmware but do
/// not make the static icon redraw.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct PowerIndicator {
    usb_present: bool,
    percentage: Option<u8>,
}

type ActionChannelType = Channel<CriticalSectionRawMutex, Action, 2>;

static ACTION: ActionChannelType = Channel::new();

const POWER_STATUS_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// Collapses a noisy voltage estimate into the six fill levels the battery
/// icon can actually render. This prevents a one-percent ADC fluctuation from
/// causing a needless status-bar repaint while the battery is not charging.
fn battery_icon_percentage(percentage: Option<u8>) -> Option<u8> {
    percentage.map(|percentage| match percentage {
        0..=5 => 5,
        6..=20 => 20,
        21..=40 => 40,
        41..=60 => 60,
        61..=80 => 80,
        _ => 100,
    })
}

impl<'a> Controller<'a> {
    pub fn new(ui: &'a WalletUi, pmu: Charger) -> Self {
        Self {
            ui,
            pmu,
            last_power_state: None,
        }
    }

    pub async fn run(&mut self) {
        self.set_action_event_handlers();

        if self.refresh_device_status().await.is_err() {
            error!("initial board-status refresh failed");
        }

        let mut power_status_ticker = Ticker::every(POWER_STATUS_REFRESH_INTERVAL);
        loop {
            match select(ACTION.receive(), power_status_ticker.next()).await {
                Either::First(action) => {
                    info!("process action {:?}", &action);
                    if self.process_action(action).await.is_err() {
                        error!("process action failed");
                    }
                }
                Either::Second(()) => {
                    if self.refresh_device_status().await.is_err() {
                        error!("periodic board-status refresh failed");
                    }
                }
            }
        }
    }

    async fn process_action(&mut self, action: Action) -> Result<(), ()> {
        match action {
            Action::RefreshDeviceStatus => self.refresh_device_status().await?,
            Action::GenerateRandomWords => self.generate_random_words()?,
        }
        Ok(())
    }

    /// Supplies the UI with a uniformly random 121-bit BIP39 prefix. The
    /// ADC-backed ESP TRNG is enabled by `main` and remains outside the UI
    /// crate, so the UI never owns or synthesizes entropy.
    fn generate_random_words(&self) -> Result<(), ()> {
        let trng = Trng::try_new().map_err(|trng_error| {
            error!("BIP39 entropy source unavailable: {trng_error:?}");
        })?;
        let word_indices: [u16; MNEMONIC_PREFIX_WORD_COUNT] =
            core::array::from_fn(|_| (trng.random() & 0x07FF) as u16);

        self.ui
            .load_mnemonic_word_indices(word_indices)
            .map_err(|ui_error| {
                // Do not log random word indices or a mnemonic phrase.
                error!("Generated BIP39 prefix was rejected: {ui_error:?}");
            })
    }

    async fn refresh_device_status(&mut self) -> Result<(), ()> {
        let battery_state = self
            .pmu
            .get_power_status()
            .await
            .map(|status| BatteryState {
                usb_present: status.usb_present,
                percentage: battery_icon_percentage(status.battery_percentage),
                charging: status.charging,
            });
        let next_state = match battery_state {
            Ok(state) => PresentedPowerState::Available(PowerIndicator {
                usb_present: state.usb_present,
                percentage: state.percentage,
            }),
            Err(e) => {
                error!("Failed to read power status: {e:?}");
                PresentedPowerState::Unavailable
            }
        };

        if self.last_power_state != Some(next_state) {
            match (next_state, battery_state) {
                (PresentedPowerState::Available(_), Ok(state)) => {
                    self.ui.set_device_status(DeviceStatus::Battery(state));
                }
                (PresentedPowerState::Unavailable, Err(_)) => {
                    self.ui.set_device_status(DeviceStatus::BatteryUnavailable);
                }
                _ => unreachable!("power presentation must match PMU read result"),
            }
            self.last_power_state = Some(next_state);
        }

        match next_state {
            PresentedPowerState::Available(_) => Ok(()),
            PresentedPowerState::Unavailable => Err(()),
        }
    }

    fn set_action_event_handlers(&self) {
        self.ui
            .on_refresh_device_status(|| send_action(Action::RefreshDeviceStatus));
        self.ui
            .on_request_random_words(|| send_action(Action::GenerateRandomWords));
    }
}

fn send_action(action: Action) {
    // The GUI callback is synchronous, so enqueue its hardware request without
    // blocking the Slint event loop.
    match ACTION.try_send(action) {
        Ok(_) => {}
        Err(action) => {
            error!("user action queue full, could not add: {action:?}")
        }
    }
}
