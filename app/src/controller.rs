use bitcoin_ui::{BatteryState, DeviceStatus, WalletUi};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use log::{error, info};

use crate::Charger;

pub struct Controller<'a> {
    ui: &'a WalletUi,
    pmu: Charger,
}

#[derive(Debug, Clone)]
enum Action {
    RefreshDeviceStatus,
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
        }
        Ok(())
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
