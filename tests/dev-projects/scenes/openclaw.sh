#!/bin/sh
# Test openclaw installation and basic usage
# https://www.npmjs.com/package/openclaw
# Note: openclaw requires Node.js v22.12+, skip on OSes with older Node.js

. "$(dirname "$0")/../common.sh"

# Install nodejs and npm first
run_install nodejs node npm

# Check Node.js version (openclaw requires v22.12+)
node_version=$(run node --version 2>/dev/null || echo "v0.0.0")
node_major=$(echo "$node_version" | sed 's/^v\([0-9]*\).*/\1/')
if [ "$node_major" -lt 22 ]; then
    lang_skip "Node.js v22+ required for openclaw (found v${node_major}.x)"
fi

# Install openclaw globally via npm (beta version)
run npm install -g openclaw@beta

# Verify installation by checking version
run openclaw --version

lang_ok
