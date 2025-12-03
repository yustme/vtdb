#!/bin/bash

# Start VTDB Web Server
# This script builds and runs the web interface

set -e

echo "Building VTDB server..."
cargo build --bin vtdb-server

echo "Starting server on http://localhost:8080..."
cargo run --bin vtdb-server

