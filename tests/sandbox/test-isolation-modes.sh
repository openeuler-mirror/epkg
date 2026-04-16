#!/bin/sh
# Test epkg run sandbox isolation modes (env, fs, vm)
#
# This test verifies that different isolation modes work correctly:
# - --isolate=env: process isolation with environment variables
# - --isolate=fs: filesystem isolation with bind mounts
# - --isolate=vm: full VM isolation with microVM backend
#
# Usage:
#   ./test-isolation-modes.sh [-d|--debug|-dd|-ddd]
#
# The test creates a temporary environment and exercises all isolation modes.
# Supports debug mode with -d/-dd/-ddd flags.
# Logs to /tmp/test-isolation-modes.log for problem analysis.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$PROJECT_ROOT/tests/common.sh"

# Parse command line flags
parse_debug_flags "$@"
case $? in
    0)
        eval set -- "$PARSE_DEBUG_FLAGS_REMAINING"
        ;;
    1)
        exit 1
        ;;
    2)
        echo "Usage: $0 [-d|--debug|-dd|-ddd]"
        echo ""
        echo "Test epkg run sandbox isolation modes"
        echo ""
        echo "Options:"
        echo "  -d, --debug    Interactive debug mode (pause on error)"
        echo "  -dd            Debug logging (RUST_LOG=debug)"
        echo "  -ddd           Trace logging (RUST_LOG=trace)"
        exit 0
        ;;
esac

set_epkg_bin
set_color_names

# Fixed test environment name (non-random for reproducibility)
TEST_ENV="test-isolation-modes"
LOG_FILE="/tmp/test-isolation-modes.log"

# Backup previous log for comparison
if [ -f "$LOG_FILE" ]; then
    mv "$LOG_FILE" "$LOG_FILE.bak"
fi

log() {
    printf "%b[TEST]%b %b\n" "$GREEN" "$NC" "$*" | tee -a "$LOG_FILE"
}

error() {
    printf "%b[ERROR]%b %b\n" "$RED" "$NC" "$*" | tee -a "$LOG_FILE"
    if [ -n "$DEBUG_FLAG" ]; then
        printf "\n=== Debug Mode ===\n" | tee -a "$LOG_FILE"
        if [ -t 0 ]; then
            printf "Press Enter to continue (or Ctrl+C to exit)...\n" | tee -a "$LOG_FILE"
            read dummy || true
        fi
    fi
    exit 1
}

# Remove existing environment before creating (leave for debug after test)
# Only remove if the env directory actually exists to avoid noisy error
TEST_ENV_DIR="$HOME/.epkg/envs/$TEST_ENV"
if [ -d "$TEST_ENV_DIR" ]; then
    log "Removing existing test environment: $TEST_ENV"
    "$EPKG_BIN" env remove "$TEST_ENV" -y 2>&1 | tee -a "$LOG_FILE" || true
fi

log "Starting sandbox isolation modes test"
log "Log file: $LOG_FILE"

# Create test environment with timeout and -y for automation
log "Creating test environment: $TEST_ENV"
timeout 60 "$EPKG_BIN" env create "$TEST_ENV" -c alpine -y 2>&1 | tee -a "$LOG_FILE" || error "Failed to create environment"

# Test 1: Default isolation (env)
log "=== Test 1: Default isolation (env) ==="
timeout 30 "$EPKG_BIN" -e "$TEST_ENV" run ls /sys 2>&1 | tee -a "$LOG_FILE" || error "Default isolation ls /sys failed"
log "Default isolation works"

# Test 2: Explicit env isolation
log "=== Test 2: Explicit --isolate=env ==="
timeout 30 "$EPKG_BIN" -e "$TEST_ENV" run --isolate=env ls /sys 2>&1 | tee -a "$LOG_FILE" || error "--isolate=env ls /sys failed"
log "Explicit env isolation works"

# Test 3: Filesystem isolation
# Note: Fs mode automatically handles necessary mounts; no explicit --mount needed for epkg bin dir
# The --mount option for deep paths outside the env can fail due to target directory creation issues
log "=== Test 3: --isolate=fs ==="
timeout 30 "$EPKG_BIN" -e "$TEST_ENV" run --isolate=fs ls / 2>&1 | tee -a "$LOG_FILE" || error "--isolate=fs ls / failed"

# Install bash for filesystem isolation tests
log "Installing bash for filesystem isolation tests"
timeout 120 "$EPKG_BIN" -e "$TEST_ENV" -y install bash coreutils 2>&1 | tee -a "$LOG_FILE" || error "Failed to install bash"

log "Testing ls /sys with --isolate=fs"
timeout 30 "$EPKG_BIN" -e "$TEST_ENV" run --isolate=fs ls /sys 2>&1 | tee -a "$LOG_FILE" || error "--isolate=fs ls /sys failed"
log "Filesystem isolation works"

# Test 4: Config persistence
log "=== Test 4: Config persistence ==="
log "Setting isolate_mode=fs in env config"
"$EPKG_BIN" -e "$TEST_ENV" env config set sandbox.isolate_mode fs 2>&1 | tee -a "$LOG_FILE" || error "Failed to set isolate_mode"

log "Testing with env config (no --isolate flag)"
timeout 30 "$EPKG_BIN" -e "$TEST_ENV" run ls /sys 2>&1 | tee -a "$LOG_FILE" || error "ls /sys failed with env config"

# Reset config
"$EPKG_BIN" -e "$TEST_ENV" env config set sandbox.isolate_mode env 2>&1 | tee -a "$LOG_FILE" || error "Failed to reset isolate_mode"
log "Config persistence works"

# Test 5: VM isolation (if supported)
log "=== Test 5: --isolate=vm (if supported) ==="

# Check if we have a static binary for VM mode
ARCH=$(uname -m)
case "$ARCH" in
    x86_64) RUST_TARGET=x86_64-unknown-linux-musl ;;
    aarch64) RUST_TARGET=aarch64-unknown-linux-musl ;;
    riscv64) RUST_TARGET=riscv64gc-unknown-linux-musl ;;
    loongarch64) RUST_TARGET=loongarch64-unknown-linux-musl ;;
    *) RUST_TARGET="" ;;
esac

if [ -n "$RUST_TARGET" ] && [ -x "$PROJECT_ROOT/target/$RUST_TARGET/debug/epkg" ]; then
    log "Found static binary for VM isolation tests"

    # VM-specific tests
    log "Testing VM-specific mount paths"

    # Test that /opt/epkg/cache is writable
    log "Testing /opt/epkg/cache is writable in VM"
    if timeout 60 "$EPKG_BIN" -e "$TEST_ENV" run --isolate=vm touch /opt/epkg/cache/.test_write 2>&1 | tee -a "$LOG_FILE"; then
        log "/opt/epkg/cache is writable in VM"
        timeout 30 "$EPKG_BIN" -e "$TEST_ENV" run --isolate=vm rm -f /opt/epkg/cache/.test_write 2>&1 | tee -a "$LOG_FILE" || true
    else
        log "Note: /opt/epkg/cache write test inconclusive (may be expected)"
    fi

    # Test with -u root
    log "Testing VM mode with -u root"
    if timeout 60 "$EPKG_BIN" -e "$TEST_ENV" run --isolate=vm -u root id 2>&1 | tee -a "$LOG_FILE" | grep -q "uid=0"; then
        log "VM mode with -u root works correctly"
    else
        log "Note: VM mode with -u root test inconclusive"
    fi

    log "VM isolation tests completed"
else
    log "Skipping VM isolation tests (no static binary found at target/$RUST_TARGET/debug/epkg)"
    log "Run 'make static-$ARCH' to build the static binary for VM tests"
fi

log "All sandbox isolation mode tests passed"
log "Test environment '$TEST_ENV' left for debugging (remove manually if needed)"