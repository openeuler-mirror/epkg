#!/bin/sh
# Test OpenAI Codex installation and basic usage
# https://www.npmjs.com/package/@openai/codex

. "$(dirname "$0")/../common.sh"

# Install nodejs and npm first
run_install nodejs node npm

# Check npm is available (symlinks may not be created properly on some distros)
check_cmd npm --version || lang_skip "npm not available (symlink issue)"

# Install Codex globally via npm
run npm install -g @openai/codex

# Verify installation by checking version
# Use full path since npm installs to /usr/local/bin which epkg may not resolve
run /usr/local/bin/codex --version

lang_ok
