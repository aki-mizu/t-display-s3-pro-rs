use bitcoin_ui::{BatteryState, MNEMONIC_PREFIX_WORD_COUNT, WalletUi};
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Ticker};
use esp_hal::rng::Trng;
use log::error;

use crate::Charger;

pub struct Controller<'a> {
    ui: &'a WalletUi,
    pmu: Charger,
    last_power_state: Option<PresentedPowerState>,
}

/// The visible power facts painted into the UI. Retaining this separately from
/// the full PMU snapshot lets the controller poll in the background without
/// repainting when no user-visible power fact changed.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum PresentedPowerState {
    Available(BatteryState),
    Unavailable,
}

type RandomWordRequestChannel = Channel<CriticalSectionRawMutex, (), 2>;

static RANDOM_WORD_REQUEST: RandomWordRequestChannel = Channel::new();

// A two-second status cadence is plenty for the compact indicator and leaves
// the shared I2C bus available for responsive touch input.
const POWER_STATUS_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

impl<'a> Controller<'a> {
    pub fn new(ui: &'a WalletUi, pmu: Charger) -> Self {
        Self {
            ui,
            pmu,
            last_power_state: None,
        }
    }

    pub async fn run(&mut self) {
        self.ui.on_request_random_words(request_random_words);

        if self.refresh_device_status().await.is_err() {
            error!("initial board-status refresh failed");
        }

        let mut power_status_ticker = Ticker::every(POWER_STATUS_REFRESH_INTERVAL);
        loop {
            match select(RANDOM_WORD_REQUEST.receive(), power_status_ticker.next()).await {
                Either::First(()) => {
                    if self.generate_random_words().is_err() {
                        error!("random BIP39 prefix generation failed");
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
        let (battery_state, next_state) = match self.pmu.get_power_status().await {
            Ok(status) => {
                let state = BatteryState {
                    usb_present: status.usb_present,
                    percentage: status.battery_percentage,
                    charging: status.charging,
                    charge_complete: status.charge_complete,
                };
                let presentation = PresentedPowerState::Available(state);
                (Ok(state), presentation)
            }
            Err(error) => {
                error!("Failed to read power status: {error:?}");
                (Err(error), PresentedPowerState::Unavailable)
            }
        };

        if self.last_power_state != Some(next_state) {
            match (next_state, battery_state) {
                (PresentedPowerState::Available(_), Ok(state)) => {
                    self.ui.set_battery_state(state);
                }
                (PresentedPowerState::Unavailable, Err(_)) => {
                    self.ui.set_battery_unavailable();
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
}

fn request_random_words() {
    // The GUI callback is synchronous, so enqueue its hardware request without
    // blocking the Slint event loop.
    match RANDOM_WORD_REQUEST.try_send(()) {
        Ok(_) => {}
        Err(_) => error!("random-word request queue full"),
    }
}
