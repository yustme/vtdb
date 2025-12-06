#!/bin/bash

# Test runner script for vtdb
# Usage: ./run-test.sh [cargo test arguments...]
# Examples:
#   ./run-test.sh                    # Run all tests
#   ./run-test.sh --test count_tests # Run specific test file
#   ./run-test.sh test_count_star    # Run specific test
#   ./run-test.sh -- --nocapture     # Run with output

set -e

echo "Running vtdb tests..."
echo ""

# If arguments are provided, pass them to cargo test
# Otherwise, run all tests
if [ $# -eq 0 ]; then
    cargo test
else
    cargo test "$@"
fi

echo ""
echo "Tests completed!"

