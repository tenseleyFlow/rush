#!/bin/sh
# =====================================
# POSIX Compliance Test Runner for Rush
# =====================================
# Runs all POSIX compliance test suites for rush
# Automatically detects rush binary location
#
# Ported from FortSH test suite

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Get script directory
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)

# Find rush binary - check multiple locations
if [ -n "$RUSH_BIN" ] && [ -x "$RUSH_BIN" ]; then
    # Use environment variable if set
    RUSH_PATH="$RUSH_BIN"
elif [ -x "$SCRIPT_DIR/../target/release/rush" ]; then
    # Look in ../target/release/rush (relative to tests/)
    RUSH_PATH="$SCRIPT_DIR/../target/release/rush"
elif [ -x "$SCRIPT_DIR/../target/debug/rush" ]; then
    # Look in ../target/debug/rush (relative to tests/)
    RUSH_PATH="$SCRIPT_DIR/../target/debug/rush"
elif [ -x "./target/release/rush" ]; then
    # Look in ./target/release/rush (from project root)
    RUSH_PATH="./target/release/rush"
elif [ -x "./target/debug/rush" ]; then
    # Look in ./target/debug/rush (from project root)
    RUSH_PATH="./target/debug/rush"
else
    printf "${RED}ERROR: rush binary not found!${NC}\n"
    printf "Searched locations:\n"
    printf "  - RUSH_BIN environment variable\n"
    printf "  - %s/../target/release/rush\n" "$SCRIPT_DIR"
    printf "  - %s/../target/debug/rush\n" "$SCRIPT_DIR"
    printf "  - ./target/release/rush\n"
    printf "  - ./target/debug/rush\n"
    printf "\nPlease build rush first with 'cargo build --release' or set RUSH_BIN\n"
    exit 1
fi

export RUSH_BIN="$RUSH_PATH"

# Test suite files (core tests - excluding huge advanced/charclass)
TEST_SUITES="
posix_compliance_test.sh
posix_compliance_extended.sh
posix_compliance_builtins.sh
posix_compliance_control.sh
posix_compliance_redirect.sh
posix_compliance_special.sh
posix_compliance_quoting.sh
posix_compliance_heredoc.sh
posix_compliance_printf.sh
posix_compliance_jobcontrol.sh
"

# Print header
printf "${CYAN}========================================\n"
printf "Rush POSIX Compliance Test Suite\n"
printf "========================================${NC}\n"
printf "rush binary: ${GREEN}%s${NC}\n" "$RUSH_BIN"
printf "Test directory: %s\n" "$SCRIPT_DIR"
printf "\n"

# Counters
TOTAL_SUITES=0
PASSED_SUITES=0
FAILED_SUITES=0

# Run each test suite
for suite in $TEST_SUITES; do
    suite_path="$SCRIPT_DIR/$suite"

    if [ ! -f "$suite_path" ]; then
        printf "${YELLOW}⊘ SKIP${NC}: %s (not found)\n" "$suite"
        continue
    fi

    if [ ! -x "$suite_path" ]; then
        printf "${YELLOW}⊘ SKIP${NC}: %s (not executable)\n" "$suite"
        continue
    fi

    TOTAL_SUITES=$((TOTAL_SUITES + 1))

    printf "${BLUE}========================================\n"
    printf "Running: %s\n" "$suite"
    printf "========================================${NC}\n"

    # Run the test suite
    if "$suite_path"; then
        printf "${GREEN}✓ PASSED${NC}: %s\n\n" "$suite"
        PASSED_SUITES=$((PASSED_SUITES + 1))
    else
        printf "${RED}✗ FAILED${NC}: %s\n\n" "$suite"
        FAILED_SUITES=$((FAILED_SUITES + 1))
    fi
done

# Print summary
printf "${CYAN}========================================\n"
printf "OVERALL SUMMARY\n"
printf "========================================${NC}\n"
printf "Total test suites: %d\n" "$TOTAL_SUITES"
printf "${GREEN}Passed suites:     %d${NC}\n" "$PASSED_SUITES"
printf "${RED}Failed suites:     %d${NC}\n" "$FAILED_SUITES"
printf "${CYAN}========================================${NC}\n"

if [ "$FAILED_SUITES" -eq 0 ] && [ "$TOTAL_SUITES" -gt 0 ]; then
    printf "${GREEN}\n✓ ALL TEST SUITES PASSED!\n${NC}"
    exit 0
else
    printf "${RED}\n✗ SOME TEST SUITES FAILED\n${NC}"
    exit 1
fi
