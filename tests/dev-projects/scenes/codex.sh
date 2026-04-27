#!/bin/sh
# Test OpenAI Codex installation and basic usage
# https://www.npmjs.com/package/@openai/codex

. "$(dirname "$0")/../common.sh"

# Install nodejs and npm first
run_install nodejs node npm

# Install Codex globally via npm
run npm install -g @openai/codex

# Verify installation by checking version
run codex --version

lang_ok
