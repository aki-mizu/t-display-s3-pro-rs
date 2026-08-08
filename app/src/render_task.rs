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
    display_line_buffer::DisplayLineBuffer, hardware::touch::read_touch,
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
    // Keep the original press point for the entire contact. Slint's
    // `TouchArea.clicked` requires release inside the pressed item; holding
    // this point makes a normal finger roll/jitter behave as one tap.
    let mut press_position: Option<LogicalPosition> = None;

    loop {
        // Update timers and animations
        slint::platform::update_timers_and_animations();

        // process touchscreen events
        if let Some(touchpad) = touchpad.as_mut() {
            process_touch(touchpad, &mut press_position, window.clone()).await;
        }

        // Draw the scene if something needs to be drawn
        let is_dirty = window.draw_if_needed(|renderer| {
            renderer.render_by_line(&mut buffer_provider);
        });

        if !is_dirty {
            // Match LilyGO's reference polling cadence closely enough to
            // catch short controller reports without busy-spinning.
            Timer::after_millis(5).await
        }
    }
}

async fn process_touch(
    touch: &mut Touchpad,
    press_position: &mut Option<LogicalPosition>,
    window: Rc<MinimalSoftwareWindow>,
) {
    // LilyGO's Pro example polls CST226SE reports. GPIO21 can be a short
    // pulse, so using it as a gate here can lose every touch before this task
    // observes it. Poll the controller directly instead of gating reads on
    // that pin.
    let point = match read_touch(touch).await {
        Ok(point) => point,
        Err(e) => {
            error!("Touch read error: {e:?}");
            // Do not leave a Slint TouchArea held if an intermittent I²C
            // transaction fails while a finger is down.
            if let Some(release_pos) = press_position.take() {
                window.dispatch_event(WindowEvent::PointerReleased {
                    position: release_pos,
                    button: PointerEventButton::Left,
                });
            }
            return;
        }
    };

    // Ignore spurious events with no reported point. Up reports are retained
    // so the renderer can release an active pointer.
    if point.points == 0 && point.event != TouchEvent::Up {
        return;
    }

    // Map touch events to Slint pointer events
    match point.event {
        TouchEvent::Up => {
            // Release at the original press location so a small finger roll
            // cannot move the release outside a compact key's TouchArea.
            if let Some(release_pos) = press_position.take() {
                window.dispatch_event(WindowEvent::PointerReleased {
                    position: release_pos,
                    button: PointerEventButton::Left,
                });
            }
        }
        TouchEvent::Contact => {
            // LilyGO's reference configuration swaps the CST226SE axes and
            // mirrors the resulting Y axis for the 480×222 landscape panel.
            let (x, y) = (point.y as f32, DISPLAY_HEIGHT as f32 - point.x as f32);
            let position = LogicalPosition::new(
                x.clamp(0.0, DISPLAY_WIDTH as f32 - 1.0),
                y.clamp(0.0, DISPLAY_HEIGHT as f32 - 1.0),
            );

            if press_position.is_none() {
                *press_position = Some(position);
                // Give Slint the same hover/press ordering as its native
                // input adapters before it evaluates a TouchArea click.
                window.dispatch_event(WindowEvent::PointerMoved { position });
                window.dispatch_event(WindowEvent::PointerPressed {
                    position,
                    button: PointerEventButton::Left,
                });
            }
        }
    }
}
