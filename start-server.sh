#!/bin/bash

# Start VTDB Web Server
# This script builds and runs the web interface

set -e

# Try to find cargo in common locations or use PATH
if ! command -v cargo &> /dev/null; then
    # Try common Rust installation paths
    if [ -f "$HOME/.cargo/env" ]; then
        source "$HOME/.cargo/env"
    elif [ -f "/Users/$USER/.cargo/env" ]; then
        source "/Users/$USER/.cargo/env"
    else
        echo "Error: cargo not found in PATH"
        echo "Please ensure Rust is installed and cargo is in your PATH"
        echo "You can install Rust from: https://rustup.rs/"
        exit 1
    fi
fi

echo "Building VTDB server..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "Build failed! Check the errors above."
    exit 1
fi

echo ""
echo "Starting VTDB server on http://localhost:8080..."
echo "Press Ctrl+C to stop the server"
echo ""

cargo run --release

