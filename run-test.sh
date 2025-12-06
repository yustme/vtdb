#!/bin/bash

# Test runner script for vtdb
# Usage: ./run-test.sh [options] [test_name]
# Examples:
#   ./run-test.sh                                    # Run all tests
#   ./run-test.sh --integration                     # Run only integration tests
#   ./run-test.sh --unit                            # Run only unit tests
#   ./run-test.sh --iceberg                         # Run Iceberg-related tests
#   ./run-test.sh --test iceberg_insert_functionality_tests  # Run specific test file
#   ./run-test.sh test_small_batch_insert_and_read  # Run specific test
#   ./run-test.sh -- --nocapture                    # Run with output

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Parse arguments
RUN_INTEGRATION=false
RUN_UNIT=false
RUN_ICEBERG=false
TEST_ARGS=()

while [[ $# -gt 0 ]]; do
    case $1 in
        --integration|-i)
            RUN_INTEGRATION=true
            shift
            ;;
        --unit|-u)
            RUN_UNIT=true
            shift
            ;;
        --iceberg)
            RUN_ICEBERG=true
            shift
            ;;
        --help|-h)
            echo "VTDB Test Runner"
            echo ""
            echo "Usage: ./run-test.sh [options] [test_name]"
            echo ""
            echo "Options:"
            echo "  --integration, -i    Run only integration tests"
            echo "  --unit, -u            Run only unit tests"
            echo "  --iceberg             Run Iceberg-related tests"
            echo "  --help, -h            Show this help message"
            echo ""
            echo "Examples:"
            echo "  ./run-test.sh                                    # Run all tests"
            echo "  ./run-test.sh --integration                     # Run integration tests"
            echo "  ./run-test.sh --iceberg                         # Run Iceberg tests"
            echo "  ./run-test.sh --test iceberg_insert_functionality_tests  # Run specific test file"
            echo "  ./run-test.sh test_small_batch_insert_and_read  # Run specific test"
            echo "  ./run-test.sh -- --nocapture                    # Run with output"
            exit 0
            ;;
        *)
            TEST_ARGS+=("$1")
            shift
            ;;
    esac
done

echo -e "${GREEN}Running VTDB tests...${NC}"
echo ""

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
    if [ -f "$HOME/.cargo/env" ]; then
        source "$HOME/.cargo/env"
    elif [ -f "/Users/$USER/.cargo/env" ]; then
        source "/Users/$USER/.cargo/env"
    else
        echo -e "${RED}Error: cargo not found in PATH${NC}"
        echo "Please ensure Rust is installed and cargo is in your PATH"
        exit 1
    fi
fi

# Build first to catch compilation errors early
echo -e "${YELLOW}Building tests...${NC}"
cargo test --no-run "${TEST_ARGS[@]}" 2>&1 | grep -E "error|warning:|Finished" || true

if [ ${PIPESTATUS[0]} -ne 0 ]; then
    echo -e "${RED}Build failed! Fix compilation errors before running tests.${NC}"
    exit 1
fi

echo ""

# Determine which tests to run
if [ "$RUN_INTEGRATION" = true ]; then
    echo -e "${GREEN}Running integration tests...${NC}"
    cargo test --test '*' "${TEST_ARGS[@]}"
elif [ "$RUN_UNIT" = true ]; then
    echo -e "${GREEN}Running unit tests...${NC}"
    cargo test --lib "${TEST_ARGS[@]}"
elif [ "$RUN_ICEBERG" = true ]; then
    echo -e "${GREEN}Running Iceberg-related tests...${NC}"
    # Run all tests matching iceberg pattern
    cargo test "${TEST_ARGS[@]}" iceberg
elif [ ${#TEST_ARGS[@]} -eq 0 ]; then
    echo -e "${GREEN}Running all tests...${NC}"
    cargo test "${TEST_ARGS[@]}"
else
    echo -e "${GREEN}Running tests with provided arguments...${NC}"
    cargo test "${TEST_ARGS[@]}"
fi

EXIT_CODE=$?

echo ""
if [ $EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}✓ All tests passed!${NC}"
else
    echo -e "${RED}✗ Some tests failed (exit code: $EXIT_CODE)${NC}"
    echo -e "${YELLOW}Check the output above for details${NC}"
fi

exit $EXIT_CODE

