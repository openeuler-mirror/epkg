#!/bin/sh
# Test opencode-ai installation and basic usage
# https://www.npmjs.com/package/opencode-ai

. "$(dirname "$0")/../common.sh"

# Install nodejs and npm first
run_install nodejs node npm

# Install opencode-ai globally via npm
run npm install -g opencode-ai

# Verify installation by checking version
run opencode --version

lang_ok
