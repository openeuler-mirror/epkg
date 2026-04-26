#!/bin/sh
# Test package manager queries (rpm/dpkg) in an epkg environment

. "$(dirname "$0")/../common.sh"

case "$OS" in
    openeuler|fedora|rhel|centos|rocky|alma) PKG_TYPE="rpm" ;;
    debian|ubuntu) PKG_TYPE="dpkg" ;;
    alpine|archlinux|conda|msys2) lang_skip "Package manager queries not supported for $OS" ;;
    *) echo "WARNING: Unknown OS type: $OS, attempting generic test" >&2; PKG_TYPE="unknown" ;;
esac

# Skip in VM mode (DB-backed tools need namespaces)
[ "${E2E_BACKEND:-}" = "vm" ] && lang_skip "Skipping in VM mode"

run_install bash

case "$PKG_TYPE" in
    rpm)
        run rpm -q -a | grep -q bash || exit 1
        run rpm -qi bash || echo "WARNING: rpm -qi bash failed" >&2
        ;;
    dpkg)
        run dpkg-query -l | grep -q '^ii.*bash' || exit 1
        run dpkg -s bash || echo "WARNING: dpkg -s bash failed" >&2
        ;;
esac

lang_ok
