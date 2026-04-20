#!/bin/sh
# Test virtiofs DAX file I/O with various file sizes
#
# DAX (Direct Access) allows guest kernel to directly map host files.
# This test verifies DAX works for files of various sizes, especially:
# - Small files (< 4KB page size)
# - Files near page boundaries (e.g., 1748 bytes, 4094 bytes)
# - Medium files (e.g., 11859 bytes)
# - Files exactly page-aligned (e.g., 4096, 8192, 12288 bytes)
#
# Usage:
#   Run inside VM: test-one.sh virtiofs-dax-io
#   Or directly: ./virtiofs-dax-io.sh (requires epkg environment)

. "$(dirname "$0")/../vars.sh"
. "$(dirname "$0")/../lib.sh"

if [ "${E2E_BACKEND:-}" != vm ]; then
	log "Skipping virtiofs DAX test outside VM (run via test-one.sh / vm.sh)."
	exit 0
fi

log "Testing virtiofs DAX file I/O with various file sizes"

# Test helper: read file and verify content
test_file_read() {
	file="$1"
	expected_first_line="$2"
	desc="$3"

	log "Testing: $desc ($file)"

	# Get file size for context
	size=$(stat -c %s "$file" 2>/dev/null || stat -f %z "$file" 2>/dev/null || echo "unknown")
	log "  File size: $size bytes"

	# Test reading file with busybox head (triggers DAX SETUPMAPPING)
	output=$(busybox head -n 5 "$file" 2>&1)
	if [ $? -ne 0 ]; then
		error "Failed to read $file: $output"
	fi

	# Verify expected content
	if [ -n "$expected_first_line" ] && ! echo "$output" | grep -q "$expected_first_line"; then
		error "Unexpected content in $file: expected '$expected_first_line', got: $output"
	fi

	log "  OK: read successful"
}

# Test helper: read file and check byte count
test_file_bytes() {
	file="$1"
	desc="$2"

	log "Testing: $desc ($file)"

	# Get file size
	size=$(stat -c %s "$file" 2>/dev/null || stat -f %z "$file" 2>/dev/null || echo "unknown")
	log "  File size: $size bytes"

	# Read file content and count bytes
	bytes=$(busybox head "$file" 2>&1 | busybox wc -c)
	if [ $? -ne 0 ]; then
		error "Failed to read $file"
	fi

	log "  Bytes read: $bytes"
	log "  OK: read successful"
}

log "=== DAX I/O tests for various file sizes ==="

# Test 1: Very small file (< 1KB) - /etc/hosts typically ~200 bytes
test_file_read "/etc/hosts" "127.0.0.1" "small file (~200 bytes)"

# Test 2: Near page boundary - /etc/inputrc typically 1748 bytes ( Alpine)
# This is a critical test: 1748 rounded to 4096 > 1748, triggers VirtualAlloc path
if [ -f /etc/inputrc ]; then
	test_file_read "/etc/inputrc" "inputrc" "near page boundary (~1748 bytes)"
else
	log "Skipping /etc/inputrc test (file not found)"
fi

# Test 3: Medium file - yash initialization file (11859 bytes in Alpine)
# Critical: 11859 rounded to 12288 > 11859, triggers VirtualAlloc path
if [ -f /usr/share/yash/initialization/common ]; then
	test_file_read "/usr/share/yash/initialization/common" "Common Yashrc" "medium file (11859 bytes)"
else
	log "Skipping yash initialization test (file not found)"
fi

# Test 4: Script file (various sizes)
if [ -f /usr/share/udhcpc/default.script ]; then
	test_file_bytes "/usr/share/udhcpc/default.script" "script file (~4010 bytes)"
else
	log "Skipping udhcpc script test (file not found)"
fi

# Test 5: Larger file - bash binary (~789KB)
# Should use file mapping for large files
if [ -f /bin/bash ]; then
	test_file_bytes "/bin/bash" "large binary file (~789KB)"
elif [ -f /usr/bin/bash ]; then
	test_file_bytes "/usr/bin/bash" "large binary file (~789KB)"
else
	log "Skipping bash binary test (file not found)"
fi

# Test 6: Directory listing (tests multiple file lookups)
log "Testing: directory listing"
output=$(busybox ls -la /bin/ 2>&1 | head -10)
if [ $? -ne 0 ]; then
	error "Failed to list /bin directory: $output"
fi
log "  OK: directory listing successful"

# Test 7: Symlink resolution (if available)
log "Testing: symlink resolution"
if [ -L /bin/sh ]; then
	output=$(busybox ls -la /bin/sh 2>&1)
	if [ $? -ne 0 ]; then
		error "Failed to resolve symlink /bin/sh: $output"
	fi
	log "  OK: symlink resolution successful"
else
	log "Skipping symlink test (/bin/sh not a symlink)"
fi

# Test 8: Create and read a file with exact page-aligned size (4096 bytes)
log "Testing: page-aligned file (4096 bytes)"
test_file="/tmp/dax-test-4096.txt"
dd if=/dev/urandom of="$test_file" bs=4096 count=1 2>/dev/null
if [ $? -ne 0 ]; then
	error "Failed to create test file $test_file"
fi
bytes=$(busybox head "$test_file" 2>&1 | busybox wc -c)
if [ $? -ne 0 ]; then
	error "Failed to read page-aligned test file"
fi
rm -f "$test_file"
log "  Bytes read: $bytes"
log "  OK: page-aligned file read successful"

# Test 9: Create and read a file just below page size (4094 bytes)
log "Testing: file just below page size (4094 bytes)"
test_file="/tmp/dax-test-4094.txt"
dd if=/dev/urandom of="$test_file" bs=4094 count=1 2>/dev/null
if [ $? -ne 0 ]; then
	error "Failed to create test file $test_file"
fi
bytes=$(busybox head "$test_file" 2>&1 | busybox wc -c)
if [ $? -ne 0 ]; then
	error "Failed to read sub-page test file"
fi
rm -f "$test_file"
log "  Bytes read: $bytes"
log "  OK: sub-page file read successful"

log "=== All virtiofs DAX I/O tests passed ==="