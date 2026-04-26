#!/bin/sh
# Test interactive/non-interactive bash and PTY in an epkg environment

. "$(dirname "$0")/../common.sh"

run_install bash coreutils

run bash -c "echo HELLO" || exit 1
run bash -c "id; whoami; pwd" || exit 1
run stat /dev/ptmx || exit 1
run ls -la /dev/pts/ || exit 1
echo "test" | run bash -c "cat" || exit 1
echo "id" | run bash || exit 1
run which bash || exit 1
run bash --version | head -1 || exit 1

lang_ok
