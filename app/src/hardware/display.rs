//! Display hardware initialization module
//!
//! This module initializes the T-Display-S3 Pro's ST7796 IPS panel via SPI
//! with DMA support.

use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_hal::delay::DelayNs;
use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
use esp_hal::dma::DmaTxBuf;
use esp_hal::gpio::{AnyPin, Level, Output, OutputConfig};
use esp_hal::peripherals::{DMA_CH0, SPI2};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config as SpiConfig, Spi, SpiDmaBus};
use esp_hal::time::Rate;
use esp_hal::{Blocking, dma_buffers};
use mipidsi::dcs::{self, InterfaceExt, SetAddressMode};
use mipidsi::interface::{Interface, InterfaceKind, SpiInterface};
use mipidsi::models::{Model, ModelInitError};
use mipidsi::options::{ColorInversion, ColorOrder, ModelOptions};
use mipidsi::options::{Orientation, Rotation};
use mipidsi::{Builder, ConfigurationError, Display};
use static_cell::StaticCell;

/// The T-Display-S3 Pro panel is physically 222×480. It is used in landscape,
/// making its logical Slint viewport 480×222.
pub const DISPLAY_HEIGHT: u16 = 222;
pub const DISPLAY_WIDTH: u16 = 480;

/// LilyGO's ST7796U panel configuration.
///
/// The T-Display-S3 Pro panel needs its vendor-specific power, timing, and
/// gamma commands. `mipidsi`'s built-in ST7796 model delegates to the much
/// shorter ST7789 initialization sequence, which leaves this panel black.
pub struct TDisplayS3ProSt7796;

impl Model for TDisplayS3ProSt7796 {
    type ColorFormat = Rgb565;

    const FRAMEBUFFER_SIZE: (u16, u16) = (320, 480);
    const RESET_DURATION: u32 = 120_000;

    fn init<DELAY, DI>(
        &mut self,
        di: &mut DI,
        delay: &mut DELAY,
        options: &ModelOptions,
    ) -> Result<SetAddressMode, ModelInitError<DI::Error>>
    where
        DELAY: DelayNs,
        DI: Interface,
    {
        if !matches!(DI::KIND, InterfaceKind::Serial4Line) {
            return Err(ModelInitError::InvalidConfiguration(
                ConfigurationError::UnsupportedInterface,
            ));
        }

        let madctl = SetAddressMode::from(options);

        // This sequence is taken from LilyGO's ST7796U reference setup for
        // the 222×480 T-Display-S3 Pro panel.
        delay.delay_us(120_000);
        di.write_command(dcs::SoftReset)?;
        delay.delay_us(120_000);
        di.write_command(dcs::ExitSleepMode)?;
        delay.delay_us(120_000);

        di.write_raw(0xF0, &[0xC3])?;
        di.write_raw(0xF0, &[0x96])?;
        di.write_command(madctl)?;
        di.write_raw(0x3A, &[0x55])?;
        di.write_raw(0xB4, &[0x01])?;
        di.write_raw(0xB6, &[0x80, 0x02, 0x3B])?;
        di.write_raw(0xE8, &[0x40, 0x8A, 0x00, 0x00, 0x29, 0x19, 0xA5, 0x33])?;
        di.write_raw(0xC1, &[0x06])?;
        di.write_raw(0xC2, &[0xA7])?;
        di.write_raw(0xC5, &[0x18])?;
        delay.delay_us(120_000);

        di.write_raw(
            0xE0,
            &[
                0xF0, 0x09, 0x0B, 0x06, 0x04, 0x15, 0x2F, 0x54, 0x42, 0x3C, 0x17, 0x14, 0x18, 0x1B,
            ],
        )?;
        di.write_raw(
            0xE1,
            &[
                0xE0, 0x09, 0x0B, 0x06, 0x04, 0x03, 0x2B, 0x43, 0x42, 0x3B, 0x16, 0x14, 0x17, 0x1B,
            ],
        )?;
        delay.delay_us(120_000);

        di.write_raw(0xF0, &[0x3C])?;
        di.write_raw(0xF0, &[0x69])?;
        di.write_command(dcs::SetInvertMode::new(options.invert_colors))?;
        di.write_command(dcs::EnterNormalMode)?;
        di.write_command(dcs::SetDisplayOn)?;
        delay.delay_us(120_000);

        Ok(madctl)
    }
}

/// ST7796U display instance using the Pro's SPI interface.
pub type TouchDisplay = Display<
    SpiInterface<
        'static,
        ExclusiveDevice<SpiDmaBus<'static, Blocking>, Output<'static>, NoDelay>,
        Output<'static>,
    >,
    TDisplayS3ProSt7796,
    Output<'static>,
>;

/// Initializes the ST7796 display with SPI interface and DMA support.
///
/// This function configures:
/// - GPIO pins for display control (DC, CS, reset, SCK, MOSI)
/// - SPI bus with DMA at 40 MHz in mode 0
/// - 222×480 active viewport, landscape orientation, BGR color order
///
/// # Arguments
///
/// * `reset` - GPIO pin for display reset
/// * `dc` - GPIO pin for data/command selection
/// * `sck` - GPIO pin for SPI clock
/// * `mosi` - GPIO pin for SPI MOSI (master out, slave in)
/// * `cs` - GPIO pin for chip select
/// * `spi` - SPI2 peripheral instance
/// * `dma` - DMA channel 0 for high-speed transfers
///
/// # Returns
///
/// Returns an initialized ST7796 display driver.
///
/// # Panics
///
/// Panics if display initialization fails.
pub fn initialize_display(
    reset: AnyPin<'static>,
    dc: AnyPin<'static>,
    sck: AnyPin<'static>,
    mosi: AnyPin<'static>,
    cs: AnyPin<'static>,
    spi: SPI2<'static>,
    dma: DMA_CH0<'static>,
) -> TouchDisplay {
    // Configure GPIO pins for display control signals (DC, CS, reset, clock, and MOSI)
    let dc = Output::new(dc, Level::Low, OutputConfig::default());
    let cs = Output::new(cs, Level::High, OutputConfig::default());
    let reset_pin = Output::new(reset, Level::High, OutputConfig::default());
    let sck = Output::new(sck, Level::Low, OutputConfig::default());
    let mosi = Output::new(mosi, Level::Low, OutputConfig::default());

    // LilyGO's pinned Arduino_GFX reference uses 40 MHz SPI mode 0.
    let spi_config = SpiConfig::default()
        .with_frequency(Rate::from_mhz(40))
        .with_mode(Mode::_0);
    let spi_dma = Spi::new(spi, spi_config)
        .unwrap()
        .with_sck(sck)
        .with_mosi(mosi)
        .with_dma(dma);

    #[allow(clippy::manual_div_ceil)]
    let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(32000);
    let dma_rx_buf = esp_hal::dma::DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
    let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

    // Create the SPI DMA bus with the configured buffers
    let spi = SpiDmaBus::new(spi_dma, dma_rx_buf, dma_tx_buf);

    // Attach the SPI device using the chip-select control pin (no delay used)
    let spi_device = ExclusiveDevice::new_no_delay(spi, cs).unwrap();

    // Allocate a buffer for display initialization commands
    static DISPLAY_BUFFER: StaticCell<[u8; 512]> = StaticCell::new();
    let buffer = DISPLAY_BUFFER.init([0_u8; 512]);

    // Create the SPI interface for the display driver using the SPI device, DC pin, and initialization buffer
    let di = SpiInterface::new(spi_device, dc, buffer);

    // The ST7796 exposes a 222×480 active crop inside its 320×480
    // framebuffer. Rotate it to the board's landscape orientation.
    Builder::new(TDisplayS3ProSt7796, di)
        .display_size(222, 480)
        .display_offset(49, 0)
        .color_order(ColorOrder::Bgr)
        .invert_colors(ColorInversion::Inverted)
        .orientation(Orientation {
            // The panel is mounted with the landscape X axis reversed. This
            // yields LilyGO TFT_eSPI's MADCTL 0x28 (MV | BGR), so rendered
            // text reads left-to-right.
            mirrored: true,
            rotation: Rotation::Deg90,
        })
        .reset_pin(reset_pin)
        .init(&mut esp_hal::delay::Delay::new())
        .expect("Failed to initialize T-Display-S3 Pro display")
}
