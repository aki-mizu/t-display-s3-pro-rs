use bitcoin_ui::{BatteryState, DeviceStatus, MNEMONIC_PREFIX_WORD_COUNT, WalletUi};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use esp_hal::rng::Trng;
use log::{error, info};

use crate::Charger;

pub struct Controller<'a> {
    ui: &'a WalletUi,
    pmu: Charger,
}

#[derive(Debug, Clone)]
enum Action {
    RefreshDeviceStatus,
    GenerateRandomWords,
}

type ActionChannelType = Channel<CriticalSectionRawMutex, Action, 2>;

static ACTION: ActionChannelType = Channel::new();

impl<'a> Controller<'a> {
    pub fn new(ui: &'a WalletUi, pmu: Charger) -> Self {
        Self { ui, pmu }
    }

    pub async fn run(&mut self) {
        self.set_action_event_handlers();

        if self.refresh_device_status().await.is_err() {
            error!("initial board-status refresh failed");
        }

        loop {
            let action = ACTION.receive().await;
            info!("process action {:?}", &action);
            if self.process_action(action).await.is_err() {
                error!("process action failed");
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
        let percentage = match self.pmu.get_battery_percentage().await {
            Ok(percentage) => percentage,
            Err(e) => {
                error!("Failed to read battery percentage: {e:?}");
                self.ui.set_device_status(DeviceStatus::BatteryUnavailable);
                return Err(());
            }
        };

        let charging = match self.pmu.is_charging().await {
            Ok(charging) => charging,
            Err(e) => {
                error!("Failed to read charging state: {e:?}");
                self.ui
                    .set_device_status(DeviceStatus::ChargerStateUnavailable { percentage });
                return Err(());
            }
        };

        self.ui
            .set_device_status(DeviceStatus::Battery(BatteryState {
                percentage,
                charging,
            }));
        Ok(())
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
