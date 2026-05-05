#!/bin/bash
# Test VM filesystem features: su command and user mapping

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$PROJECT_ROOT/tests/common.sh"

OS_TYPE=$(uname -s)
VMM_BACKEND="libkrun"

# Parse arguments
for arg in "$@"; do
    case "$arg" in
        --vmm=*) VMM_BACKEND="${arg#--vmm=}" ;;
    esac
done
parse_debug_flags "$@"
case $? in
    0) ;;
    1) exit 1 ;;
    2) echo "Usage: $0 [--vmm=libkrun] [-d]"; exit 0 ;;
esac

case "$DEBUG_FLAG" in
    -ddd) export RUST_LOG=trace RUST_BACKTRACE=1; set -x ;;
    -dd)  export RUST_LOG=debug RUST_BACKTRACE=1 ;;
    -d)   export RUST_LOG=debug ;;
esac

set_epkg_bin
set_color_names

ENV_NAME="test-vm-su-env"

log() { printf "%b[TEST]%b %s\n" "$GREEN" "$NC" "$*" >&2; }
error() { printf "%b[ERROR]%b %s\n" "$RED" "$NC" "$*" >&2; "$EPKG_BIN" env remove "$ENV_NAME" 2>/dev/null; exit 1; }
skip() { printf "%b[SKIP]%b %s\n" "$YELLOW" "$NC" "$*" >&2; exit 0; }

case "$OS_TYPE" in
    Linux*)
        [ ! -e /dev/kvm ] && skip "KVM not available"
        [ ! -r /dev/kvm ] && skip "No read permission on /dev/kvm"
        [ ! -w /dev/kvm ] && skip "No write permission on /dev/kvm"
        ;;
    *) skip "Linux only" ;;
esac

log "Setup: creating environment"
"$EPKG_BIN" env remove "$ENV_NAME" 2>/dev/null || true
"$EPKG_BIN" env create "$ENV_NAME" -c alpine || error "env create failed"
"$EPKG_BIN" -e "$ENV_NAME" --assume-yes install busybox-static || error "install failed"

# Test 1: su works for non-root user
log "Test 1: Non-root user via su"
output=$("$EPKG_BIN" -e "$ENV_NAME" run --isolate=vm --vmm="$VMM_BACKEND" --io=batch \
    sh -c 'adduser -D testuser; /usr/bin/busybox.static su -s /bin/sh testuser -c id' 2>&1)
echo "$output" | grep -q "testuser" && log "Test 1: PASSED" || error "Test 1 FAILED: $output"

# Test 2: VM root is environment root
log "Test 2: VM root shows environment directories"
output=$("$EPKG_BIN" -e "$ENV_NAME" run --isolate=vm --vmm="$VMM_BACKEND" --io=batch \
    sh -c 'ls /' 2>&1)
echo "$output" | grep -q "bin" && log "Test 2: PASSED" || error "Test 2 FAILED: $output"

# Test 3: non-root can traverse root (the key fix verification)
log "Test 3: Non-root user can traverse root directory"
output=$("$EPKG_BIN" -e "$ENV_NAME" run --isolate=vm --vmm="$VMM_BACKEND" --io=batch \
    sh -c 'adduser -D testuser 2>/dev/null; /usr/bin/busybox.static su -s /bin/sh testuser -c "test -d /bin && echo TRAVERSE_OK"' 2>&1)
echo "$output" | grep -q "TRAVERSE_OK" && log "Test 3: PASSED" || error "Test 3 FAILED: $output"

# Test 4: host UID mapping
log "Test 4: Host UID appears as root in VM"
output=$("$EPKG_BIN" -e "$ENV_NAME" run --isolate=vm --vmm="$VMM_BACKEND" --io=batch \
    sh -c 'stat -c "%u %g" /usr/bin/busybox.static' 2>&1)
echo "$output" | grep -q "0 0" && log "Test 4: PASSED" || error "Test 4 FAILED: $output"

log "Cleanup"
"$EPKG_BIN" env remove "$ENV_NAME" 2>/dev/null || true

log "All tests PASSED"