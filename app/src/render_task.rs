use alloc::rc::Rc;
use embassy_time::Timer;
use log::error;
use slint::{
    LogicalPosition,
    platform::{
        PointerEventButton, WindowEvent,
        software_renderer::{MinimalSoftwareWindow, Rgb565Pixel},
    },
};

use crate::{
    DISPLAY_HEIGHT, DISPLAY_WIDTH, TouchDisplay, TouchEvent, Touchpad,
    display_line_buffer::DisplayLineBuffer,
    hardware::touch::{is_touch_available, read_touch},
};

#[embassy_executor::task()]
pub async fn render_task(
    window: Rc<MinimalSoftwareWindow>,
    display: TouchDisplay,
    mut touchpad: Option<Touchpad>,
) {
    // Initialize buffer provider
    let line_buffer = &mut [Rgb565Pixel(0); DISPLAY_WIDTH as usize];

    let mut buffer_provider = DisplayLineBuffer::new(display, line_buffer);
    let mut last_touch: Option<LogicalPosition> = None;

    loop {
        // Update timers and animations
        slint::platform::update_timers_and_animations();

        // process touchscreen events
        if let Some(touchpad) = touchpad.as_mut() {
            process_touch(touchpad, &mut last_touch, window.clone()).await;
        }

        // Draw the scene if something needs to be drawn
        let is_dirty = window.draw_if_needed(|renderer| {
            renderer.render_by_line(&mut buffer_provider);
        });

        if !is_dirty {
            Timer::after_millis(10).await
        }
    }
}

async fn process_touch(
    touch: &mut Touchpad,
    last_touch: &mut Option<LogicalPosition>,
    window: Rc<MinimalSoftwareWindow>,
) {
    // Treat a transient touch-bus fault as a missed input instead of taking
    // down the renderer (and the visible display) with a panic.
    let touch_available = match is_touch_available(touch) {
        Ok(available) => available,
        Err(error) => {
            error!("Touch availability check failed: {error:?}");
            return;
        }
    };
    if !touch_available {
        return;
    }

    // Read the touch data
    let point = match read_touch(touch).await {
        Ok(point) => point,
        Err(e) => {
            error!("Touch read error: {e:?}");
            return;
        }
    };

    // Ignore spurious events with no reported point. Up reports are retained
    // so the renderer can release an active pointer.
    if point.points == 0 && point.event != TouchEvent::Up {
        return;
    }

    // LilyGO's reference configuration swaps the CST226SE axes and mirrors
    // the resulting Y axis when the 222×480 panel is used in landscape.
    let (x, y) = (point.y as f32, DISPLAY_HEIGHT as f32 - point.x as f32);
    let x = x.clamp(0.0, DISPLAY_WIDTH as f32 - 1.0);
    let y = y.clamp(0.0, DISPLAY_HEIGHT as f32 - 1.0);
    let position = LogicalPosition::new(x, y);

    // Map touch events to Slint pointer events
    match point.event {
        TouchEvent::Up => {
            // Use the last tracked position for reliable release when a finger
            // leaves the screen or the CST226SE returns an empty report.
            if let Some(release_pos) = last_touch.take() {
                window.dispatch_event(WindowEvent::PointerReleased {
                    position: release_pos,
                    button: PointerEventButton::Left,
                });
            }
        }
        TouchEvent::Contact => {
            let event = if last_touch.replace(position).is_some() {
                WindowEvent::PointerMoved { position }
            } else {
                WindowEvent::PointerPressed {
                    position,
                    button: PointerEventButton::Left,
                }
            };
            window.dispatch_event(event);
        }
    }
}
