#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

// Import core allocator utilities and modules
extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

use alloc::boxed::Box;
use bitcoin_ui::WalletUi;
use controller::Controller;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embedded_graphics_core::{draw_target::DrawTarget, pixelcolor::Rgb565};
use esp_alloc::psram_allocator;
use esp_backtrace as _;
use esp_hal::Async;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{AnyPin, Level, Output, OutputConfig, Pin};
use esp_hal::i2c::master::I2c;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::peripherals::I2C0;
use esp_hal::rng::TrngSource;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use log::{error, info};
use render_task::render_task;
use slint::PhysicalSize;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint_backend::Backend;
use static_cell::StaticCell;

// Hardware initialization modules
mod hardware;
use hardware::*;

mod controller;
mod display_line_buffer;
mod render_task;
mod slint_backend;

/// Main entry point for the application
#[esp_rtos::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();

    // Initialize peripherals and configure the CPU clock
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::_240MHz));

    // Keep ADC-backed hardware entropy enabled for the entire application.
    // The BIP39 UI requests random word indices through the controller; it
    // never receives raw random bytes or owns this board-specific source.
    let trng_source = TrngSource::new(peripherals.RNG, peripherals.ADC1);
    info!("ADC-backed TRNG enabled for BIP39 generation");

    // Reserve memory for dynamic allocations
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    // Set up the timer group and software interrupt for the embassy executor
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);
    info!("Embassy initialized!");

    // Bring up the PMU before the display and touch controller add their
    // startup load.
    let i2c_bus = initialize_i2c(
        peripherals.I2C0,
        peripherals.GPIO5.degrade(),
        peripherals.GPIO6.degrade(),
    );
    let pmu = initialize_pmu(I2cDevice::new(i2c_bus)).await;

    // LilyGO specifies OPI PSRAM for the T-Display-S3 Pro.
    psram_allocator!(
        peripherals.PSRAM,
        esp_hal::psram,
        esp_hal::psram::PsramConfig {
            mode: esp_hal::psram::PsramMode::OctalSpi,
            ..Default::default()
        }
    );

    // The Pro's IPS panel needs its dedicated GPIO48 backlight enabled.
    let mut _backlight = Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default());
    _backlight.set_high();
    info!("T-Display-S3 Pro backlight enabled");

    // Bring the display up before optional I2C peripherals. A bright splash
    // remains visible if a later peripheral fails during board bring-up.
    let mut display = initialize_display(
        peripherals.GPIO47.degrade(),
        peripherals.GPIO9.degrade(),
        peripherals.GPIO18.degrade(),
        peripherals.GPIO17.degrade(),
        peripherals.GPIO39.degrade(),
        peripherals.SPI2,
        peripherals.DMA_CH0,
    );
    display
        .clear(Rgb565::new(0, 63, 0))
        .expect("Failed to draw display bring-up splash");

    // This BIP39-only firmware does not use the camera hardware. Keep its
    // sensor powered down and torch off to avoid an unnecessary USB-only load.
    let _camera_torch = Output::new(peripherals.GPIO38, Level::Low, OutputConfig::default());
    let _camera_power_down = Output::new(peripherals.GPIO46, Level::High, OutputConfig::default());

    // Create the GUI window for Slint's minimal software renderer
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    let size = PhysicalSize::new(DISPLAY_WIDTH.into(), DISPLAY_HEIGHT.into());
    window.set_size(size);

    // Set up the Slint rendering platform backend
    let backend = Box::new(Backend::new(window.clone()));
    slint::platform::set_platform(backend).expect("set_platform failed");

    // Initialize the touchpad interface for user interactions.
    let touchpad = match initialize_touchpad(
        I2cDevice::new(i2c_bus),
        peripherals.GPIO21.degrade(),
        peripherals.GPIO13.degrade(),
    )
    .await
    {
        Ok(touchpad) => Some(touchpad),
        Err(touch_error) => {
            error!(
                "CST226SE touch initialization failed: {touch_error:?}; continuing without touch input"
            );
            None
        }
    };

    // Launch the GUI render task asynchronously
    spawner.spawn(render_task(window, display, touchpad).expect("Unable to spawn render task"));

    // Create and show the application window UI
    let ui = WalletUi::new().expect("UI init failed");
    ui.show().expect("UI show failed");

    // Start the main event loop in the controller with the UI and PMU
    let mut controller = Controller::new(&ui, pmu);
    controller.run().await;

    // Retain the ADC entropy source until every short-lived `Trng` user from
    // the controller has been dropped.
    drop(trng_source);
}

/// Type alias for the shared I2C bus wrapped in a Mutex
type SharedI2cBus = Mutex<CriticalSectionRawMutex, I2c<'static, Async>>;

/// Initialize the I2C bus used to communicate with external devices.
/// Returns a shared I2C bus wrapped in a Mutex for safe concurrent access.
fn initialize_i2c(
    i2c: I2C0<'static>,
    sda: AnyPin<'static>,
    scl: AnyPin<'static>,
) -> &'static SharedI2cBus {
    // Create a new I2C master instance with default configuration
    // The SY6970 and touch controller share this bus. Use the conservative
    // 40 kHz rate proven reliable on this board.
    let i2c = I2c::new(
        i2c,
        esp_hal::i2c::master::Config::default().with_frequency(Rate::from_khz(40)),
    )
    .unwrap()
    .with_sda(sda)
    .with_scl(scl)
    .into_async();

    // Wrap it in a Mutex for sharing between devices
    static I2C_BUS: StaticCell<Mutex<CriticalSectionRawMutex, I2c<'static, Async>>> =
        StaticCell::new();
    I2C_BUS.init(Mutex::new(i2c))
}
