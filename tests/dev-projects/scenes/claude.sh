#!/bin/sh
# Test Anthropic Claude Code installation and basic usage
# https://www.npmjs.com/package/@anthropic-ai/claude-code

. "$(dirname "$0")/../common.sh"

# Install nodejs and npm first
run_install nodejs node npm

# Install Claude Code globally via npm
run npm install -g @anthropic-ai/claude-code

# Verify installation by checking version
run claude --version

lang_ok
