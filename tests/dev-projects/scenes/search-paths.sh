#!/bin/sh
# Test epkg search --paths functionality in an epkg environment

. "$(dirname "$0")/../common.sh"

case "$OS" in
    openeuler|fedora|debian|ubuntu) ;;
    *) lang_skip "File list search not tested for $OS (only openeuler/fedora/debian tested)" ;;
esac

run_install bash

# search --paths may download file list database (time consuming)
"$EPKG_BIN" -e "$ENV_NAME" search --paths /bin/bash >/dev/null 2>&1 || exit 1

result=$("$EPKG_BIN" -e "$ENV_NAME" search --paths /nonexistent/path/12345 2>&1)
echo "$result" | grep -q "not found\|no matches\|No package" || echo "WARNING: Unexpected output for non-existent path: $result" >&2

lang_ok
