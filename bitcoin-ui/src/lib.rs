#![no_std]

//! Board-independent touchscreen presentation layer for the BIP39 final-word helper.
//!
//! This crate owns the generated Slint window and its local-only word-entry
//! flow. It deliberately has no ESP32, PMU, key, seed-derivation, signing, or
//! networking dependencies.

extern crate alloc;

mod generated;
mod mnemonic;
mod mnemonic_view;
mod wallet_ui;

pub use wallet_ui::{BatteryState, UiError, WalletUi};

/// Number of words supplied to the final-word calculator.
pub const MNEMONIC_PREFIX_WORD_COUNT: usize = bip39_last_word::PREFIX_WORD_COUNT;
