//! Hardware initialization modules
//!
//! This module provides organized initialization functions for all hardware
//! components used by the LilyGO T-Display-S3 Pro:
//!
//! - **Display**: ST7796 IPS controller via SPI with DMA
//! - **Touchpad**: CST226SE capacitive touch controller via I2C
//! - **PMU**: SY6970 battery charger via I2C
//! - **Camera Shield**: GPIO power, MCLK, and SCCB probing

pub mod camera;
pub mod display;
pub mod pmu;
pub mod touch;

// Re-export commonly used types and functions for convenience
pub use camera::{initialize_camera_shield, probe_camera_sensor};
pub use display::{DISPLAY_HEIGHT, DISPLAY_WIDTH, TouchDisplay, initialize_display};
pub use pmu::{Charger, initialize_pmu};
pub use touch::{TouchEvent, Touchpad, initialize_touchpad};
