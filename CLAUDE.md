# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with this repository.

## Project Overview

This is an embedded Rust application for the LilyGO T-Display-S3 Pro with optional Camera Shield support. It integrates the ST7796 display, CST226SE touch controller, SY6970 power-management IC, camera-shield control, and a Slint GUI.

**Target platform:** ESP32-S3 (`xtensa-esp32s3-none-elf`)
**Toolchain:** ESP Rust toolchain (`channel = "esp"`)
**Main dependencies:** `esp-hal`, `esp-rtos`, Embassy, and Slint

## Code Quality

Run these after making changes:

```bash
cargo fmt
cargo clippy
```

## Build Commands

Build and flash require the ESP toolchain environment (`source ~/export-esp.sh`).

```bash
# Release build
./scripts/build.sh

# Debug build
./scripts/build.sh debug

# Equivalent direct Cargo commands
cargo build -p app --bin app --release
cargo build -p app --bin app
```

## Flash to Device

```bash
# Build, flash, and reset the board
./scripts/flash.sh

# Debug build, flash, and reset
./scripts/flash.sh debug
```

The flash script selects DIO flash mode at 80 MHz, which the board requires.

## Documentation

```bash
# Generate and open application documentation
cargo doc -p app --no-deps --open

# Generate and open driver documentation
cargo doc -p drivers --no-deps --open
```

## Project Architecture

### Workspace Structure

This Cargo workspace has three members:

1. **`app/`** — the ESP32-S3 application binary
2. **`drivers/`** — hardware driver library, including the CST226SE touch and SY6970 PMU drivers
3. **`slint-generated/`** — Slint UI compilation package

### Core Application Architecture

The application uses Embassy's asynchronous task executor:

- **`main.rs`** initializes board hardware and starts application tasks.
- **`controller.rs`** processes UI actions through Embassy channels.
- **`render_task.rs`** renders Slint line-by-line and processes touch input.
- **`hardware/camera.rs`** configures Camera Shield power, clock, and sensor probing.

UI callbacks are synchronous while the controller is asynchronous. `send_action()` bridges this boundary with non-blocking `try_send()`; an action is logged and dropped when the channel is full.

### Hardware Modules

Hardware modules live in `app/src/hardware/`:

- **`display.rs`** drives the ST7796 panel through SPI with DMA, using a 480×222 landscape logical viewport.
- **`touch.rs`** accesses CST226SE touch input over I²C.
- **`pmu.rs`** accesses the SY6970 battery charger and power-management IC over I²C.
- **`camera.rs`** configures the optional Camera Shield and provides SCCB sensor detection; image capture is sensor-specific and is not yet implemented.

All I²C devices share a bus through `embassy_embedded_hal::shared_bus` with mutex protection.

### Display and UI

- **UI framework:** Slint software renderer (RGB565)
- **UI files:** `slint-generated/ui/`
- **Build process:** Slint files compile through `slint-generated/build.rs`
- **Rendering:** `DisplayLineBuffer` renders one line at a time to limit memory use
- **Touch mapping:** coordinates are transformed to match the landscape display orientation; see `render_task.rs`

### Memory Configuration

- **Heap:** custom DRAM allocation (73,744 bytes in `.dram2_uninit`)
- **PSRAM:** Octal/OPI PSRAM configured through `esp_hal::psram::PsramConfig`
- **Profile:** debug builds use optimization level `s`, because unoptimized ESP32 builds are too slow

## Important Implementation Details

### Touch Event Handling

- The first contact is sent to Slint as a pointer press; later reports for
  that contact are sent as pointer moves.
- Up events use the last tracked position so a release remains reliable when a finger leaves the screen.
- Coordinates are clamped and transformed before sending them to Slint.

### Driver Features

The `drivers` crate exposes asynchronous register access through its `async` feature.

### Linker Configuration

`app/build.rs` supplies linker configuration and helpful errors for common setup problems. The `-Tlinkall.x` linker script must be last.

## Development Environment Setup

1. Install the ESP Rust toolchain and `espflash`.
2. Ensure `~/export-esp.sh` exports the ESP toolchain environment.
3. Use the ESP toolchain's nightly Rust support for `static_cell` nightly features.

## Hardware Notes

- I²C: SDA GPIO5, SCL GPIO6
- Display SPI2: RST GPIO47, DC GPIO9, SCK GPIO18, MOSI GPIO17, CS GPIO39, BL GPIO48
- Touch: IRQ GPIO21, RST GPIO13
- The board uses OPI PSRAM and DIO flash.
- Camera Shield: PWDN GPIO46, torch GPIO38, VSYNC GPIO7, HREF GPIO15, XCLK GPIO11, PCLK GPIO2, D0…D7 GPIO45/GPIO41/GPIO40/GPIO42/GPIO1/GPIO3/GPIO10/GPIO4
- Camera sensor capture and preview need an exact sensor/module identification before a capture pipeline can be added.
- The SPI driver is placed in RAM for performance (`ESP_HAL_PLACE_SPI_DRIVER_IN_RAM`).
