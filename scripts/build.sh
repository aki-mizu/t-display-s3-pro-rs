#!/usr/bin/env bash

set -e

source ~/export-esp.sh >/dev/null 2>&1

case "${1:-}" in
"" | "release")
    cargo build -p app --bin app --release
    ;;
"debug")
    cargo build -p app --bin app
    ;;
*)
    echo "Wrong argument. Use debug or release." >&2
    exit 1
    ;;
esac
