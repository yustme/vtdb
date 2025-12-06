#!/bin/bash

# Script to watch for changes in app.js and automatically update the version
# Usage: ./watch-js-changes.sh
# Requires: fswatch (install via: brew install fswatch on macOS, or apt-get install fswatch on Linux)

set -e

WEB_DIR="web"
APP_JS="${WEB_DIR}/app.js"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Watching for changes in ${APP_JS}..."
echo "Press Ctrl+C to stop watching."

# Check if fswatch is available
if ! command -v fswatch &> /dev/null; then
    echo "Error: fswatch is not installed."
    echo "Install it with:"
    echo "  macOS: brew install fswatch"
    echo "  Linux: sudo apt-get install fswatch"
    exit 1
fi

# Watch for changes and update version when app.js changes
fswatch -o "$APP_JS" | while read f; do
    echo "Detected change in app.js, updating version..."
    "$SCRIPT_DIR/update-js-version.sh"
done

