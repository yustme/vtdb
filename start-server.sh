#!/bin/bash

# Start VTDB Web Server
# This script builds and runs the web interface

set -e

echo "Building VTDB server..."
cargo build --release

echo "Starting VTDB server on http://localhost:8080..."
echo "Press Ctrl+C to stop the server"
echo ""

cargo run --release

