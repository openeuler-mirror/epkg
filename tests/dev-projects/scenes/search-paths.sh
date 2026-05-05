#!/bin/sh
# Test epkg search --paths functionality in an epkg environment

. "$(dirname "$0")/../common.sh"

case "$OS" in
    openeuler|fedora|debian|ubuntu) ;;
    *) lang_skip "File list search not tested for $OS (only openeuler/fedora/debian tested)" ;;
esac

run_install bash

# search --paths may download file list database (time consuming)
"$EPKG_BIN" -e "$ENV_NAME" search --paths /bin/bash || exit 1

result=$("$EPKG_BIN" -e "$ENV_NAME" search --paths /nonexistent/path/12345 2>&1)
# Empty output or "not found"/"no matches"/"No package" are all valid "no results" responses
echo "$result" | grep -q "not found\|no matches\|No package" || [ -z "$result" ] || echo "WARNING: Unexpected output for non-existent path: $result" >&2

lang_ok
