#![no_std]

//! Board-independent touchscreen presentation and address-derivation layer.
//!
//! This crate owns the generated Slint window, local-only word-entry flow, and
//! BIP84 receive-address presentation. It deliberately has no ESP32, PMU,
//! persistence, signing, or networking dependencies.

extern crate alloc;

mod address_list;
mod generated;
mod mnemonic;
mod mnemonic_view;
mod wallet_ui;

pub use wallet_ui::{BatteryState, UiError, WalletUi};

/// Number of words supplied to the final-word calculator.
pub const MNEMONIC_PREFIX_WORD_COUNT: usize = bip39_last_word::PREFIX_WORD_COUNT;
