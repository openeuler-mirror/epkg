#!/bin/sh
# Test opencode-ai installation and basic usage
# https://www.npmjs.com/package/opencode-ai
# Note: opencode-ai's postinstall may create a wrong .opencode cache (glibc on musl).
# If .opencode fails to execute, delete it so the script finds the correct platform binary.

. "$(dirname "$0")/../common.sh"

# Install nodejs and npm first
run_install nodejs node npm

# Install opencode-ai globally via npm
run npm install -g opencode-ai

# Check if .opencode cache can execute; if not, delete it so script finds correct platform binary
# This handles the case where opencode-ai installs glibc binary on musl systems
run sh -c 'CACHE=/usr/local/lib/node_modules/opencode-ai/bin/.opencode; [ -f "$CACHE" ] && ! "$CACHE" --version >/dev/null 2>&1 && rm "$CACHE"'

# Verify installation by checking version
run opencode --version

lang_ok
