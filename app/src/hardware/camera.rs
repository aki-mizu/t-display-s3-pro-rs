//! Optional Camera Shield support for the T-Display-S3 Pro.
//!
//! LilyGO sells the board with more than one DVP camera module.  The shared
//! wiring and power controls are stable, but image capture requires a
//! sensor-specific SCCB initialization sequence.  This module therefore keeps
//! the shield powered safely, supplies its master clock, and reports a
//! responding SCCB address without claiming that every shield is the same
//! sensor.

use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embedded_hal_async::i2c::I2c as _;
use esp_hal::Async;
use esp_hal::gpio::{AnyPin, Level, Output, OutputConfig};
use esp_hal::i2c::master::I2c;
use esp_hal::lcd_cam::{
    LcdCam,
    cam::{Camera, Config},
};
use esp_hal::peripherals::{DMA_CH1, LCD_CAM};
use esp_hal::time::Rate;

/// A powered Camera Shield. Keep this value alive for as long as the camera
/// should remain enabled.
pub struct CameraShield {
    // Retaining the camera interface keeps its 20 MHz MCLK running on GPIO11.
    // A sensor-specific driver can later use this same interface for DVP DMA
    // capture instead of claiming LCD_CAM a second time.
    _interface: Camera<'static>,
    // LilyGO warns against driving this LED directly high. Keep the pin low
    // until a PWM-backed brightness API is added.
    _torch: Output<'static>,
    _power_down: Output<'static>,
}

/// Initializes the Camera Shield's power-down GPIO and keeps its torch off.
///
/// The DVP data bus itself is deliberately not claimed here: it can only be
/// used after programming the installed camera sensor through SCCB.
pub fn initialize_camera_shield(
    torch: AnyPin<'static>,
    power_down: AnyPin<'static>,
    xclk: AnyPin<'static>,
    lcd_cam: LCD_CAM<'static>,
    dma: DMA_CH1<'static>,
) -> CameraShield {
    // PWDN is active-high. Leave the sensor powered so an SCCB probe and a
    // future LCD_CAM capture task can use it.
    let power_down = Output::new(power_down, Level::Low, OutputConfig::default());
    let interface = Camera::new(
        LcdCam::new(lcd_cam).cam,
        dma,
        Config::default().with_frequency(Rate::from_mhz(20)),
    )
    .expect("Failed to configure Camera Shield master clock")
    .with_master_clock(xclk);
    let torch = Output::new(torch, Level::Low, OutputConfig::default());
    CameraShield {
        _interface: interface,
        _torch: torch,
        _power_down: power_down,
    }
}

/// Probes common SCCB addresses used by LilyGO-compatible Camera Shields.
///
/// This only establishes electrical presence. It intentionally does not guess
/// a sensor model or attempt a capture, since GC0308, OV2640, OV5640, and
/// other modules need different register initialization sequences.
pub async fn probe_camera_sensor(
    i2c: &mut I2cDevice<'static, CriticalSectionRawMutex, I2c<'static, Async>>,
) -> Option<u8> {
    // 0x21 is common for GC0308; 0x30 and 0x3c are common OV-series SCCB
    // addresses. Probe only with an address phase, which does not mutate the
    // device state.
    for address in [0x21, 0x30, 0x3c] {
        if i2c.write(address, &[]).await.is_ok() {
            return Some(address);
        }
    }
    None
}
