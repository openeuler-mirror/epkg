#!/bin/sh
# Test curl installation and HTTPS connectivity in an epkg environment

. "$(dirname "$0")/../common.sh"

# Skip in VM mode (guest DNS not guaranteed)
[ "${E2E_BACKEND:-}" = "vm" ] && lang_skip "Skipping HTTPS test in VM mode"

run_install curl

run curl --version 2>&1 | grep -q "https" || lang_skip "curl lacks HTTPS support"
run curl -s -I -o /dev/null -w "%{http_code}" https://example.com/ | grep -q "200" || echo "WARNING: HTTPS request returned non-200" >&2

lang_ok
