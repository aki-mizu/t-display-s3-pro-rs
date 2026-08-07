# LilyGO T-Display-S3 Pro Rust Application

An embedded Rust application for the LilyGO T-Display-S3 Pro and its optional Camera Shield. It uses the ST7796 display, CST226SE capacitive touch controller, SY6970 power-management IC, OPI PSRAM, and a Slint-based user interface.

## Features

- **Display**: ST7796 IPS panel (222×480 physical, 480×222 landscape UI) over SPI
- **Touch**: CST226SE capacitive touch controller over I²C
- **Power**: SY6970 battery charger and power-management IC
- **Camera Shield**: sensor power, 20 MHz camera clock, a safely disabled torch, and SCCB sensor probing
- **Bitcoin UI demo**: compact account, receive, and PSBT-review screens sized for the 480×222 landscape display
- **UI**: Slint embedded GUI in the board-independent `bitcoin-ui` crate, with asynchronous rendering in `app`

Camera capture and preview are not implemented yet. Camera Shields can ship with different image sensors, so capture requires a sensor-specific SCCB initialization sequence and DVP capture configuration.

## Bitcoin Demo Safety

The Bitcoin screens are a local interface demonstration, not a wallet. Their balances, transaction review, and receive address are deliberately sample data. The firmware has no private keys, wallet persistence, transaction signing, network access, QR import, or PSBT parsing. Do not send funds to the displayed example address.

The UI is an original board-sized implementation. It does not include KeyOS source code, assets, or dependencies.

## Quick Start

Install the ESP Rust toolchain and source its environment before building:

```bash
source ~/export-esp.sh

# Release build
./scripts/build.sh

# Flash and reset a release build
./scripts/flash.sh
```

Use `debug` for a debug build or flash:

```bash
./scripts/build.sh debug
./scripts/flash.sh debug
```

The flash script uses the board's required DIO flash mode at 80 MHz.

## Board Wiring

| Peripheral | Mapping |
| --- | --- |
| I²C | SDA GPIO5, SCL GPIO6 |
| SPI2 display | RST GPIO47, DC GPIO9, SCK GPIO18, MOSI GPIO17, CS GPIO39, BL GPIO48 |
| Touch | IRQ GPIO21, RST GPIO13 |
| Camera Shield control | PWDN GPIO46, torch GPIO38 |
| Camera Shield DVP | VSYNC GPIO7, HREF GPIO15, XCLK GPIO11, PCLK GPIO2, D0…D7 GPIO45/GPIO41/GPIO40/GPIO42/GPIO1/GPIO3/GPIO10/GPIO4 |

The torch pin remains off until PWM brightness control is implemented, following the board vendor's guidance against driving it directly high.

## Development

For build instructions, architecture notes, and development guidelines, see [CLAUDE.md](./CLAUDE.md).

The `bitcoin-ui` crate owns Slint components, screen navigation, UI actions, and presentation-state setters. The `app` crate owns the ESP32-S3 backend, display/touch integration, and hardware action handling.

## Contributing

Contributions are welcome. Please open an issue before making major changes.

## License

MIT License – see [LICENSE](./LICENSE) for details.
