#![no_std]

//! Board-independent touchscreen presentation layer for the BIP39 final-word helper.
//!
//! This crate owns the generated Slint window and its local-only word-entry
//! flow. It deliberately has no ESP32, PMU, key, seed-derivation, signing, or
//! networking dependencies.

extern crate alloc;

use alloc::{rc::Rc, string::String};
use bip39_last_word::{
    Error as Bip39Error, PREFIX_WORD_COUNT, word_for_entropy_bits, word_for_index, word_index,
    words_by_prefix,
};
use core::cell::RefCell;
use slint::ComponentHandle;

mod generated {
    slint::include_modules!();
}

use generated::AppWindow;

/// Number of words supplied to the final-word calculator.
pub const MNEMONIC_PREFIX_WORD_COUNT: usize = PREFIX_WORD_COUNT;

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
/// Screen navigation and user-facing strings remain in Slint or this wrapper.
/// Firmware supplies typed board facts and fulfils explicit requests that need
/// board hardware, such as fresh entropy.
pub struct WalletUi {
    window: AppWindow,
    mnemonic_flow: Rc<RefCell<LastWordFlow>>,
}

impl WalletUi {
    /// Creates the compact 480 by 222 Bitcoin demo window.
    pub fn new() -> Result<Self, UiError> {
        let window = AppWindow::new().map_err(|_| UiError)?;
        let mnemonic_flow = configure_mnemonic_flow(&window);

        Ok(Self {
            window,
            mnemonic_flow,
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
        self.mnemonic_flow
            .borrow_mut()
            .load_word_indices(word_indices)?;
        self.window.set_current_screen(1);
        sync_mnemonic_view(&self.window, &self.mnemonic_flow.borrow());
        Ok(())
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

const MAX_WORD_PREFIX_LEN: usize = 4;

/// Ephemeral touchscreen state for the BIP39 final-word helper.
///
/// The completed prefix remains only as word-list indices. It never builds or
/// stores a concatenated mnemonic phrase.
#[derive(Default)]
struct LastWordFlow {
    word_indices: [u16; PREFIX_WORD_COUNT],
    word_count: usize,
    prefix: [u8; MAX_WORD_PREFIX_LEN],
    prefix_len: usize,
    last_letter_was_rejected: bool,
    entropy_bits: u8,
    entropy_confirmed: bool,
}

impl LastWordFlow {
    fn push_letter(&mut self, key: i32) {
        if self.is_complete() || self.prefix_len == MAX_WORD_PREFIX_LEN {
            return;
        }

        let Ok(letter_offset) = u8::try_from(key) else {
            return;
        };
        if letter_offset >= 26 {
            return;
        }

        let letter = b'a' + letter_offset;
        let mut candidate_prefix = self.prefix;
        candidate_prefix[self.prefix_len] = letter;
        let candidate_len = self.prefix_len + 1;
        let Ok(candidate) = core::str::from_utf8(&candidate_prefix[..candidate_len]) else {
            return;
        };

        if words_by_prefix(candidate).is_empty() {
            // Do not add an impossible spelling to the prefix, but make the
            // deliberate BIP39 filter visible so it is not mistaken for a
            // missed touchscreen tap.
            self.last_letter_was_rejected = true;
            return;
        }

        self.prefix = candidate_prefix;
        self.prefix_len = candidate_len;
        self.last_letter_was_rejected = false;
    }

    fn backspace(&mut self) {
        self.last_letter_was_rejected = false;

        if self.prefix_len > 0 {
            self.prefix_len -= 1;
            self.prefix[self.prefix_len] = 0;
            return;
        }

        if self.word_count > 0 {
            self.word_count -= 1;
            self.word_indices[self.word_count] = 0;
            self.clear_entropy();
        }
    }

    fn commit_word(&mut self) -> bool {
        let Some(index) = self.selected_word_index() else {
            return false;
        };
        if self.is_complete() {
            return false;
        }

        self.word_indices[self.word_count] = index;
        self.word_count += 1;
        self.prefix = [0; MAX_WORD_PREFIX_LEN];
        self.prefix_len = 0;
        self.last_letter_was_rejected = false;
        self.clear_entropy();

        self.is_complete()
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    /// Replaces the complete eleven-word prefix only after every index has
    /// been checked, so an invalid board input cannot partially change it.
    fn load_word_indices(
        &mut self,
        word_indices: [u16; PREFIX_WORD_COUNT],
    ) -> Result<(), Bip39Error> {
        for (position, index) in word_indices.iter().copied().enumerate() {
            if word_for_index(index).is_none() {
                return Err(Bip39Error::InvalidWordIndex { position, index });
            }
        }

        self.word_indices = word_indices;
        self.word_count = PREFIX_WORD_COUNT;
        self.prefix = [0; MAX_WORD_PREFIX_LEN];
        self.prefix_len = 0;
        self.clear_entropy();
        Ok(())
    }

    fn toggle_entropy_bit(&mut self, bit: i32) {
        if !self.is_complete() {
            return;
        }

        let Ok(bit) = u32::try_from(bit) else {
            return;
        };
        if bit >= 7 {
            return;
        }

        self.entropy_bits ^= 1_u8 << bit;
        self.entropy_confirmed = false;
    }

    fn confirm_entropy(&mut self) {
        if self.is_complete() {
            // This explicit action is required even when the selected value is
            // 0000000. Eleven words alone do not select a unique final word.
            self.entropy_confirmed = true;
        }
    }

    fn is_complete(&self) -> bool {
        self.word_count == PREFIX_WORD_COUNT
    }

    fn prefix_text(&self) -> &str {
        core::str::from_utf8(&self.prefix[..self.prefix_len]).unwrap_or("")
    }

    fn selected_word_index(&self) -> Option<u16> {
        let prefix = self.prefix_text();
        if prefix.is_empty() {
            return None;
        }

        if let Some(index) = word_index(prefix) {
            return Some(index);
        }

        let matches = words_by_prefix(prefix);
        (matches.len() == 1)
            .then(|| word_index(matches[0]))
            .flatten()
    }

    fn entry_text(&self) -> String {
        let prefix = self.prefix_text();
        if prefix.is_empty() {
            return String::from("Tap letters to filter the English word list.");
        }

        let matches = words_by_prefix(prefix);
        if let Some(index) = word_index(prefix) {
            return word_for_index(index)
                .map(|word| alloc::format!("Use exact: {word}"))
                .unwrap_or_default();
        }
        if matches.len() == 1 {
            return alloc::format!("Only match: {}", matches[0]);
        }

        alloc::format!("{} matches — add a letter.", matches.len())
    }

    fn message(&self) -> String {
        if self.last_letter_was_rejected {
            return String::from(
                "No BIP39 word can continue with that letter. Try another letter.",
            );
        }

        if self.is_complete() {
            return String::from("All 11 words captured. Select the seven entropy bits.");
        }

        alloc::format!(
            "Word {} of {PREFIX_WORD_COUNT}. Tap Use word to confirm it.",
            self.word_count + 1
        )
    }

    fn entropy_bits_label(&self) -> String {
        let mut label = String::with_capacity(7);
        for bit in (0..7).rev() {
            label.push(if self.entropy_bits & (1_u8 << bit) == 0 {
                '0'
            } else {
                '1'
            });
        }
        label
    }

    fn entropy_bit_is_set(&self, bit: u8) -> bool {
        self.entropy_bits & (1_u8 << bit) != 0
    }

    fn candidate_word(&self) -> Option<&'static str> {
        self.entropy_confirmed
            .then(|| word_for_entropy_bits(&self.word_indices, self.entropy_bits).ok())
            .flatten()
    }

    fn candidate_index(&self) -> Option<u8> {
        self.entropy_confirmed.then_some(self.entropy_bits)
    }

    fn clear_entropy(&mut self) {
        self.entropy_bits = 0;
        self.entropy_confirmed = false;
    }
}

fn configure_mnemonic_flow(window: &AppWindow) -> Rc<RefCell<LastWordFlow>> {
    let flow = Rc::new(RefCell::new(LastWordFlow::default()));
    sync_mnemonic_view(window, &flow.borrow());

    let weak_window = window.as_weak();
    window.on_mnemonic_key({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move |key| {
            flow.borrow_mut().push_letter(key);
            if let Some(window) = weak_window.upgrade() {
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    window.on_mnemonic_backspace({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move || {
            flow.borrow_mut().backspace();
            if let Some(window) = weak_window.upgrade() {
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    window.on_mnemonic_commit({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move || {
            // A completed prefix always leads to the final-word screen. This
            // also gives a visible recovery path if a repaint was interrupted
            // immediately after the eleventh confirmation.
            let complete = {
                let mut flow = flow.borrow_mut();
                if flow.is_complete() {
                    true
                } else {
                    flow.commit_word()
                }
            };
            if let Some(window) = weak_window.upgrade() {
                if complete {
                    window.set_current_screen(1);
                }
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    window.on_mnemonic_reset({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move || {
            flow.borrow_mut().reset();
            if let Some(window) = weak_window.upgrade() {
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    window.on_entropy_toggle({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move |bit| {
            flow.borrow_mut().toggle_entropy_bit(bit);
            if let Some(window) = weak_window.upgrade() {
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    window.on_entropy_confirm({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move || {
            flow.borrow_mut().confirm_entropy();
            if let Some(window) = weak_window.upgrade() {
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    flow
}

fn sync_mnemonic_view(window: &AppWindow, flow: &LastWordFlow) {
    window.set_mnemonic_word_count(flow.word_count as i32);
    window.set_mnemonic_prefix(flow.prefix_text().into());
    window.set_mnemonic_entry(flow.entry_text().into());
    window.set_mnemonic_message(flow.message().into());
    window.set_mnemonic_can_commit(flow.selected_word_index().is_some());
    window.set_entropy_bits_label(flow.entropy_bits_label().into());
    window.set_entropy_bit_64(flow.entropy_bit_is_set(6));
    window.set_entropy_bit_32(flow.entropy_bit_is_set(5));
    window.set_entropy_bit_16(flow.entropy_bit_is_set(4));
    window.set_entropy_bit_8(flow.entropy_bit_is_set(3));
    window.set_entropy_bit_4(flow.entropy_bit_is_set(2));
    window.set_entropy_bit_2(flow.entropy_bit_is_set(1));
    window.set_entropy_bit_1(flow.entropy_bit_is_set(0));
    window.set_candidate_word(flow.candidate_word().unwrap_or("").into());
    window.set_candidate_index(flow.candidate_index().map(i32::from).unwrap_or(0));
}

#[cfg(test)]
mod mnemonic_tests {
    use super::*;

    fn enter_word(flow: &mut LastWordFlow, word: &str) {
        for byte in word.bytes().take(MAX_WORD_PREFIX_LEN) {
            flow.push_letter(i32::from(byte - b'a'));
        }
        assert!(flow.selected_word_index().is_some());
        flow.commit_word();
    }

    #[test]
    fn permits_exact_short_words_and_keeps_indices_only() {
        let mut flow = LastWordFlow::default();
        for byte in b"act" {
            flow.push_letter(i32::from(*byte - b'a'));
        }

        assert_eq!(flow.selected_word_index(), word_index("act"));
        assert!(!flow.commit_word());
        assert_eq!(flow.word_indices[0], word_index("act").unwrap());
    }

    #[test]
    fn makes_an_impossible_next_letter_explicit() {
        let mut flow = LastWordFlow::default();
        flow.push_letter(0); // a
        flow.push_letter(0); // aa is not a BIP39 word prefix

        assert_eq!(flow.prefix_text(), "a");
        assert!(flow.last_letter_was_rejected);
        assert!(flow.message().contains("No BIP39 word"));

        flow.push_letter(1); // ab is a valid prefix, for example "abandon"
        assert_eq!(flow.prefix_text(), "ab");
        assert!(!flow.last_letter_was_rejected);
    }

    #[test]
    fn requires_explicit_entropy_confirmation_for_zero() {
        let mut flow = LastWordFlow::default();
        for _ in 0..PREFIX_WORD_COUNT {
            enter_word(&mut flow, "abandon");
        }

        assert!(flow.is_complete());
        assert_eq!(flow.candidate_word(), None);
        flow.confirm_entropy();
        assert_eq!(flow.candidate_word(), Some("about"));
        assert_eq!(flow.candidate_index(), Some(0));
    }

    #[test]
    fn changing_entropy_hides_the_previous_result() {
        let mut flow = LastWordFlow::default();
        for _ in 0..PREFIX_WORD_COUNT {
            enter_word(&mut flow, "abandon");
        }
        flow.confirm_entropy();
        assert_eq!(flow.candidate_word(), Some("about"));

        flow.toggle_entropy_bit(0);
        assert_eq!(flow.candidate_word(), None);
    }

    #[test]
    fn the_eleventh_confirmation_completes_the_prefix() {
        let mut flow = LastWordFlow::default();
        for _ in 0..(PREFIX_WORD_COUNT - 1) {
            enter_word(&mut flow, "abandon");
        }

        for byte in b"aban" {
            flow.push_letter(i32::from(*byte - b'a'));
        }
        assert!(flow.commit_word());
        assert!(flow.is_complete());
    }

    #[test]
    fn loading_indices_replaces_the_prefix_and_clears_entropy() {
        let mut flow = LastWordFlow::default();
        flow.word_indices[0] = 123;
        flow.word_count = 1;
        flow.prefix[..3].copy_from_slice(b"act");
        flow.prefix_len = 3;
        flow.entropy_bits = 0b101_0101;
        flow.entropy_confirmed = true;

        let indices = core::array::from_fn(|position| position as u16);
        assert_eq!(flow.load_word_indices(indices), Ok(()));

        assert_eq!(flow.word_indices, indices);
        assert!(flow.is_complete());
        assert_eq!(flow.prefix_text(), "");
        assert_eq!(flow.entropy_bits, 0);
        assert!(!flow.entropy_confirmed);
        assert_eq!(flow.candidate_word(), None);
    }

    #[test]
    fn invalid_loaded_index_does_not_partially_replace_the_prefix() {
        let mut flow = LastWordFlow::default();
        flow.word_indices[0] = 123;
        flow.word_count = 1;
        flow.prefix[..3].copy_from_slice(b"act");
        flow.prefix_len = 3;

        let mut indices = [0; PREFIX_WORD_COUNT];
        indices[7] = 2048;
        assert_eq!(
            flow.load_word_indices(indices),
            Err(Bip39Error::InvalidWordIndex {
                position: 7,
                index: 2048,
            })
        );

        assert_eq!(flow.word_indices[0], 123);
        assert_eq!(flow.word_count, 1);
        assert_eq!(flow.prefix_text(), "act");
    }

    #[test]
    fn selects_the_official_all_one_entropy_vector() {
        let words = [
            "legal", "winner", "thank", "year", "wave", "sausage", "worth", "useful", "legal",
            "winner", "thank",
        ];
        let mut flow = LastWordFlow::default();
        for word in words {
            enter_word(&mut flow, word);
        }
        for bit in 0..7 {
            flow.toggle_entropy_bit(bit);
        }

        flow.confirm_entropy();
        assert_eq!(flow.candidate_word(), Some("yellow"));
    }

    #[test]
    fn slint_key_callback_updates_the_prefix() {
        use alloc::{boxed::Box, rc::Rc};
        use slint::{
            PhysicalSize,
            platform::{
                Platform, PlatformError, WindowAdapter,
                software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
            },
        };

        struct TestPlatform(Rc<MinimalSoftwareWindow>);

        impl Platform for TestPlatform {
            fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
                Ok(self.0.clone())
            }

            fn duration_since_start(&self) -> core::time::Duration {
                core::time::Duration::ZERO
            }
        }

        let renderer_window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        slint::platform::set_platform(Box::new(TestPlatform(renderer_window.clone()))).ok();
        renderer_window.set_size(PhysicalSize::new(480, 222));
        let window = AppWindow::new().expect("create test window");
        configure_mnemonic_flow(&window);

        window.invoke_mnemonic_key(0);

        assert_eq!(window.get_mnemonic_prefix().as_str(), "a");
    }
}
