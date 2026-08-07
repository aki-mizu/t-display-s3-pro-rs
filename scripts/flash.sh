#!/usr/bin/env bash

set -e

source ~/export-esp.sh >/dev/null 2>&1

ESPFLASH_RUNNER="espflash flash --after hard-reset -c esp32s3 -s 16mb -m dio -f 80mhz --no-skip"

case "${1:-}" in
"" | "release")
    CARGO_TARGET_XTENSA_ESP32S3_NONE_ELF_RUNNER="$ESPFLASH_RUNNER" \
        cargo run -p app --bin app --release
    ;;
"debug")
    CARGO_TARGET_XTENSA_ESP32S3_NONE_ELF_RUNNER="$ESPFLASH_RUNNER" \
        cargo run -p app --bin app
    ;;
*)
    echo "Wrong argument. Use debug or release." >&2
    exit 1
    ;;
esac
