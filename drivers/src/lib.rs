#![no_std]
//! Drivers for LilyGO T-Display-S3 Pro peripherals.
//!
//! Provides the SY6970 PMU and CST226SE touch controller drivers.

/// SY6970 battery charging and power path management IC driver.
pub mod sy6970;

/// CST226SE capacitive touch sensor driver.
pub mod cst226se;
