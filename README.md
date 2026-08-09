# LilyGO T-Display-S3 Pro Rust Application

An embedded Rust BIP39 final-word helper for the LilyGO T-Display-S3 Pro. It uses the ST7796 display, CST226SE capacitive touch controller, SY6970 power-management IC, OPI PSRAM, and a Slint-based user interface.

## Features

- **Display**: ST7796 IPS panel (222×480 physical, 480×222 landscape UI) over SPI
- **Touch**: CST226SE capacitive touch controller over I²C
- **Power**: SY6970 battery charger and power-management IC
- **BIP39 helper**: local entry of eleven mnemonic words and calculation of valid final-word choices
- **UI**: Slint embedded GUI in the board-independent `bitcoin-ui` crate, with asynchronous rendering in `app`

## BIP39 Helper Safety

The BIP39 helper is not a wallet. It does not derive keys, persist a mnemonic, sign transactions, access a network, import QR data, or parse PSBTs.

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
| Unused camera controls | PWDN GPIO46 held high, torch GPIO38 held low |

The camera controls are held in this safe state to avoid an unnecessary USB-only power load.

## Development

For build instructions, architecture notes, and development guidelines, see [CLAUDE.md](./CLAUDE.md).

The `bip39-last-word` crate owns the BIP39 calculation. The `bitcoin-ui` crate owns Slint components and UI state; the `app` crate owns ESP32-S3 display, touch, power, and entropy integration.

## Contributing

Contributions are welcome. Please open an issue before making major changes.

## License

MIT License – see [LICENSE](./LICENSE) for details.
