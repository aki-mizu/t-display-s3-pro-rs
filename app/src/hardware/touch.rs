//! Touch hardware initialization and board-independent touch events.
//!
//! The T-Display-S3 Pro uses a CST226SE. Keep its controller-specific protocol
//! here and expose a small common event shape to the renderer.

use drivers::cst226se::CST226SE;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Delay;
use esp_hal::Async;
use esp_hal::gpio::{AnyPin, Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::i2c::master::I2c;
use log::info;

/// A controller-independent contact state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchEvent {
    Up,
    Contact,
}

/// A controller-independent first touch point.
#[derive(Debug, Clone, Copy)]
pub struct TouchPoint {
    pub points: u8,
    pub event: TouchEvent,
    pub x: u16,
    pub y: u16,
}

/// Errors surfaced to the render task. Controller-specific errors are logged
/// at the call site but do not need to leak into the UI pipeline.
#[derive(Debug, Clone, Copy)]
pub struct TouchpadError;

/// Type alias for the T-Display-S3 Pro's CST226SE touch controller.
pub type Touchpad = CST226SE<
    I2cDevice<'static, CriticalSectionRawMutex, I2c<'static, Async>>,
    Input<'static>,
    Output<'static>,
    Delay,
>;

/// Initializes the Pro's CST226SE capacitive touch controller.
pub async fn initialize_touchpad(
    i2c_device: I2cDevice<'static, CriticalSectionRawMutex, I2c<'static, Async>>,
    touch: AnyPin<'static>,
    reset: AnyPin<'static>,
) -> Result<Touchpad, TouchpadError> {
    let touch_pin = Input::new(touch, InputConfig::default().with_pull(Pull::None));
    let reset_pin = Output::new(reset, Level::High, OutputConfig::default());
    let mut touchpad = CST226SE::new(i2c_device, touch_pin, Some(reset_pin), Delay);
    touchpad.begin().await.map_err(|_| TouchpadError)?;
    info!("Initialized CST226SE touchpad at I2C address 0x5A");
    Ok(touchpad)
}

/// Returns whether the controller has asserted its active-low interrupt.
pub fn is_touch_available(touchpad: &mut Touchpad) -> Result<bool, TouchpadError> {
    touchpad.is_touch_available().map_err(|_| TouchpadError)
}

/// Reads one touch report using the selected board's controller protocol.
pub async fn read_touch(touchpad: &mut Touchpad) -> Result<TouchPoint, TouchpadError> {
    match touchpad.read_touch().await.map_err(|_| TouchpadError)? {
        Some(point) => Ok(TouchPoint {
            points: point.points,
            // The CST226SE report does not provide a portable down/contact
            // mapping. The renderer derives it from its previous state.
            event: TouchEvent::Contact,
            x: point.x,
            y: point.y,
        }),
        None => Ok(TouchPoint {
            points: 0,
            event: TouchEvent::Up,
            x: 0,
            y: 0,
        }),
    }
}
