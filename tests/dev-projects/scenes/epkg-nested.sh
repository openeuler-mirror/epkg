#!/bin/sh
# Test nested epkg/apk/apt/dnf commands via bash in an epkg environment
#
# This test verifies that:
# - epkg can be invoked from within a bash command inside an environment
# - The output matches between direct and nested invocations
# - Nested epkg/apk/apt/dnf correctly use the same environment
#
# Usage:
#   E2E_OS=debian ./test-epkg-nested.sh [-d|--debug|-dd|-ddd]
#   ./test-epkg-nested.sh debian [-d|--debug|-dd|-ddd]

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
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
        echo "Usage: $0 [OS] [-d|--debug|-dd|-ddd]"
        echo ""
        echo "Test nested epkg/apk/apt/dnf commands via bash"
        echo ""
        echo "Arguments:"
        echo "  OS              Target OS/distro (default: from E2E_OS env var, or debian)"
        echo ""
        echo "Options:"
        echo "  -d, --debug    Interactive debug mode"
        echo "  -dd            Debug logging"
        echo "  -ddd           Trace logging"
        echo ""
        echo "Environment:"
        echo "  E2E_OS          Target OS/distro"
        exit 0
        ;;
esac

set_epkg_bin
set_color_names

# Determine target OS (skip conda - runs in host OS, not environment)
TARGET_OS="${1:-${E2E_OS:-debian}}"
if [ "$TARGET_OS" = "conda" ]; then
    warn "Skipping test for conda (epkg run runs in host OS, not environment)"
    exit 0
fi

TEST_ENV="test-nested-${TARGET_OS}-$$"

log "Starting nested command test for OS: $TARGET_OS"
log "Test environment: $TEST_ENV"

cleanup() {
    if [ -n "$TEST_ENV" ]; then
        log "Cleaning up environment: $TEST_ENV"
        "$EPKG_BIN" --assume-yes env remove "$TEST_ENV" 2>/dev/null || true
    fi
}

trap cleanup EXIT INT TERM

# Create environment
log "Creating environment for $TARGET_OS"
"$EPKG_BIN" env remove "$TEST_ENV" 2>/dev/null || true
"$EPKG_BIN" env create "$TEST_ENV" -c "$TARGET_OS" || error "Failed to create environment"

# Install bash
log "Installing bash"
"$EPKG_BIN" -e "$TEST_ENV" --assume-yes install --no-install-essentials bash || error "Failed to install bash"

# inside_env: run a shell command inside the test environment
# Uses 'epkg run' to execute bash -c inside the environment.
inside_env() { "$EPKG_BIN" -e "$TEST_ENV" run bash -c "$*"; }

# ---------------------------------------------------------------------------
# Compare direct vs nested epkg list output (core fields only)
#
# Note: repo data may differ between host and VM (VM may auto-update repo).
# Compare only core fields (pkgname, version, arch, size, depth), ignoring
# repo and upgrade markers.
# ---------------------------------------------------------------------------
log "Comparing direct vs nested epkg list output (core fields only)"

# Extract core fields: depth, size, name, version, arch (columns 2,3,4,5,6)
# Format: |I| depth | size | name | version | arch | repo | summary
# Strip status flags (I/U), keep depth, size, name, version, arch
extract_core_fields() {
    # Input line: "I       4  405.57 KB  musl  1.2.5-r21  aarch64  main  description"
    # Output:     "4  405.57 KB  musl  1.2.5-r21  aarch64"
    sed 's/^[IU]*[[:space:]]*//' | awk '{print $1, $2, $3, $4, $5, $6}'
}

list1=$("$EPKG_BIN" -e "$TEST_ENV" list 2>/dev/null | tail -n +5 | grep -v '^Total' | grep -v '^$' | extract_core_fields | sort)

# On macOS (VM mode), EPKG_ACTIVE_ENV is set so `epkg list` auto-detects the env.
# On Linux, pass -e explicitly so the scope matches the direct (host-side) command.
case "$(uname -s)" in
    Darwin)
        list2=$(inside_env "epkg list" 2>/dev/null | tail -n +5 | grep -v '^Total' | grep -v '^$' | extract_core_fields | sort)
        ;;
    *)
        list2=$(inside_env "epkg -e \"$TEST_ENV\" list" 2>/dev/null | tail -n +5 | grep -v '^Total' | grep -v '^$' | extract_core_fields | sort)
        ;;
esac

if [ "$list1" != "$list2" ]; then
    log "ERROR: epkg list core fields differ between direct and nested"
    diff_tmp1=$(mktemp)
    diff_tmp2=$(mktemp)
    echo "$list1" > "$diff_tmp1"
    echo "$list2" > "$diff_tmp2"
    diff -u "$diff_tmp1" "$diff_tmp2" >&2 || true
    rm -f "$diff_tmp1" "$diff_tmp2"
    error "Output mismatch between direct and nested epkg list"
fi
log "Direct and nested epkg list outputs match (core fields)"

# ---------------------------------------------------------------------------
# Test nested package management via epkg, apk, apt, and dnf CLI shims
#
# Each entry is "cmd:list_subcmd:install_subcmd:remove_subcmd"
#   epkg:  list / install / remove
#   apk:   list / add     / del
#   apt:   list / install / remove
#   dnf:   list / install / remove (prints extra "Installed packages:" header)
#
# All four CLIs now support -y/--assume-yes for non-interactive operation.
# ---------------------------------------------------------------------------
for entry in "epkg:list:install:remove" \
             "apk:list:add:del" \
             "apt:list:install:remove" \
             "dnf:list:install:remove"; do
    cmd=$(echo "$entry" | cut -d: -f1)
    list_subcmd=$(echo "$entry" | cut -d: -f2)
    install_subcmd=$(echo "$entry" | cut -d: -f3)
    remove_subcmd=$(echo "$entry" | cut -d: -f4)

    assume_yes="-y"

    log "Testing $cmd commands inside environment"

    # Test list
    log "  Testing $cmd $list_subcmd"
    if ! inside_env "$cmd $list_subcmd" >/dev/null 2>&1; then
        error "$cmd $list_subcmd failed"
    fi
    log "  $cmd $list_subcmd works"

    # Test install tree
    log "  Testing $cmd $assume_yes $install_subcmd tree"
    if ! inside_env "$cmd $assume_yes $install_subcmd tree" >/dev/null 2>&1; then
        error "$cmd $install_subcmd tree failed"
    fi
    log "  $cmd $install_subcmd tree works"

    # Verify tree appears in list output
    log "  Verifying tree appears in $cmd $list_subcmd output"
    if ! inside_env "$cmd $list_subcmd" 2>/dev/null | grep -qw tree; then
        error "Package tree not found after $cmd $install_subcmd"
    fi
    log "  $cmd install verification passed"

    # Test remove tree
    log "  Testing $cmd $assume_yes $remove_subcmd tree"
    if ! inside_env "$cmd $assume_yes $remove_subcmd tree" >/dev/null 2>&1; then
        error "$cmd $remove_subcmd tree failed"
    fi
    log "  $cmd $remove_subcmd tree works"
done

log "All nested command tests passed for $TARGET_OS"
