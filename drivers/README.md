# T-Display-S3 Pro drivers

This crate contains the board-specific peripheral drivers used by the
LilyGO T-Display-S3 Pro application:

- `cst226se`: async capacitive-touch driver at I²C address `0x5A`.
- `sy6970`: async battery charger and power-path driver at I²C address `0x6A`.

Both drivers use `embedded-hal` traits. Enable the `async` feature to use
their asynchronous APIs, as the application does.

Generate API documentation with:

```bash
cargo doc -p drivers --no-deps --open
```
