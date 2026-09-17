#!/usr/bin/env bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ -f "$SCRIPT_DIR/target/release/ctrl-cli" ]; then
    exec "$SCRIPT_DIR/target/release/ctrl-cli" tui "$@"
elif [ -f "$SCRIPT_DIR/ctrl-cli/target/release/ctrl-cli" ]; then
    exec "$SCRIPT_DIR/ctrl-cli/target/release/ctrl-cli" tui "$@"
elif [ -f "$SCRIPT_DIR/target/debug/ctrl-cli" ]; then
    exec "$SCRIPT_DIR/target/debug/ctrl-cli" tui "$@"
elif [ -f "$SCRIPT_DIR/ctrl-cli/target/debug/ctrl-cli" ]; then
    exec "$SCRIPT_DIR/ctrl-cli/target/debug/ctrl-cli" tui "$@"
elif [ -f "$SCRIPT_DIR/Cargo.toml" ]; then
    exec cargo run --manifest-path "$SCRIPT_DIR/Cargo.toml" --bin ctrl-cli -- tui "$@"
else
    exec cargo run --manifest-path "$SCRIPT_DIR/ctrl-cli/Cargo.toml" --bin ctrl-cli -- tui "$@"
fi
