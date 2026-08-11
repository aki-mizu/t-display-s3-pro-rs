use alloc::rc::Rc;
use bip39_last_word::{Error as Bip39Error, PREFIX_WORD_COUNT};
use core::cell::RefCell;
use slint::ComponentHandle;

use crate::{
    address_list::ReceiveAddressCache,
    generated::AppWindow,
    mnemonic::LastWordFlow,
    mnemonic_view::{configure_mnemonic_flow, sync_mnemonic_view},
};

/// A failure returned while creating or showing the Slint UI.
#[derive(Debug)]
pub struct UiError;

/// Power facts supplied by board firmware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryState {
    /// Whether an external USB input is connected to the PMU.
    pub usb_present: bool,
    /// A voltage-derived battery percentage. `None` means no usable battery
    /// was detected, such as when the physical battery switch is off.
    pub percentage: Option<u8>,
    /// Whether the PMU is actively pre-charging or fast-charging the battery.
    pub charging: bool,
    /// Whether the compact indicator should present a static full battery.
    /// This is mutually exclusive with [`Self::charging`].
    pub charge_complete: bool,
}

/// The public, board-independent interface to the Bitcoin demo window.
///
/// Screen navigation and user-facing strings remain in Slint or this wrapper.
/// Firmware supplies typed board facts and fulfils explicit requests that need
/// board hardware, such as fresh entropy.
pub struct WalletUi {
    window: AppWindow,
    mnemonic_flow: Rc<RefCell<LastWordFlow>>,
    receive_address_cache: Rc<RefCell<Option<ReceiveAddressCache>>>,
}

impl WalletUi {
    /// Creates the compact 480 by 222 Bitcoin demo window.
    pub fn new() -> Result<Self, UiError> {
        let window = AppWindow::new().map_err(|_| UiError)?;
        let receive_address_cache = Rc::new(RefCell::new(None));
        let mnemonic_flow = configure_mnemonic_flow(&window, Rc::clone(&receive_address_cache));

        Ok(Self {
            window,
            mnemonic_flow,
            receive_address_cache,
        })
    }

    /// Makes the window visible through the configured Slint platform.
    pub fn show(&self) -> Result<(), UiError> {
        self.window.show().map_err(|_| UiError)
    }

    /// Registers a board-supplied source of random BIP39 prefix words.
    ///
    /// The UI never generates entropy itself. Firmware should use a hardware
    /// random source, then call [`Self::load_mnemonic_word_indices`].
    pub fn on_request_random_words(&self, handler: impl Fn() + 'static) {
        self.window.on_request_random_words(handler);
    }

    /// Replaces the current prefix with eleven validated BIP39 word indices.
    ///
    /// This deliberately accepts indices rather than a phrase, keeping the
    /// UI's mnemonic state fixed-size and avoiding any assembled seed phrase.
    pub fn load_mnemonic_word_indices(
        &self,
        word_indices: [u16; PREFIX_WORD_COUNT],
    ) -> Result<(), Bip39Error> {
        self.receive_address_cache.borrow_mut().take();
        self.mnemonic_flow
            .borrow_mut()
            .load_word_indices(word_indices)?;
        self.window.set_current_screen(1);
        sync_mnemonic_view(&self.window, &self.mnemonic_flow.borrow());
        Ok(())
    }

    /// Updates the compact battery indicator from board-supplied facts.
    pub fn set_battery_state(&self, state: BatteryState) {
        let percentage = state.percentage.unwrap_or(0).min(100);
        self.window.set_usb_present(state.usb_present);
        self.window.set_battery_present(state.percentage.is_some());
        self.window.set_battery_percentage(i32::from(percentage));
        self.window.set_battery_charging(state.charging);
        self.window.set_charge_complete(state.charge_complete);
    }

    /// Hides the compact battery indicator until a later PMU read succeeds.
    pub fn set_battery_unavailable(&self) {
        self.window.set_battery_present(false);
        self.window.set_usb_present(false);
        self.window.set_battery_percentage(0);
        self.window.set_battery_charging(false);
        self.window.set_charge_complete(false);
    }
}
