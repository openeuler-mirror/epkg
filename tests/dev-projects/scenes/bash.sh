#!/bin/sh
# Test bash installation and /bin/sh usability in an epkg environment

. "$(dirname "$0")/../common.sh"

run_install bash

run /bin/sh -c 'exit 0' || exit 1
run /bin/sh -c 'echo hello-from-sh' 2>/dev/null | grep -q "hello-from-sh" || exit 1
"$EPKG_BIN" -e "$ENV_NAME" info bash >/dev/null 2>&1 || exit 1

lang_ok
