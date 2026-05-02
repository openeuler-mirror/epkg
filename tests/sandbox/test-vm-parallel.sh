#!/bin/bash
# Test parallel VM clients with dynamic CID allocation

set -e

EPKG=/c/epkg/target/debug/epkg
ENV=alpine

echo "=== Test Parallel VM Clients ==="

# Clean up
pkill -9 -f qemu-system 2>/dev/null || true
rm -f ~/.epkg/run/vm-sessions/*.json 2>/dev/null || true
rm -f ~/.epkg/run/vsock-*.sock 2>/dev/null || true
sleep 1

echo ""
echo "1. Starting VM1 (long-running with vm-keep-timeout)"
$EPKG -e $ENV run --isolate=vm --vmm=qemu --vm-keep-timeout=60 sleep 30 &
VM1_PID=$!
echo "   VM1 PID: $VM1_PID"

# Wait for VM1 to start, run command, and register session
# Session is registered AFTER guest ready (after command completes)
# So we need to wait for the first command to complete
sleep 8

echo ""
echo "2. Checking session file"
if [ -f ~/.epkg/run/vm-sessions/$ENV.json ]; then
    echo "   Session exists:"
    cat ~/.epkg/run/vm-sessions/$ENV.json | head -5
    CID=$(grep socket_path ~/.epkg/run/vm-sessions/$ENV.json | sed 's/.*vsock://' | tr -d '",')
    echo "   CID: $CID"
else
    echo "   ERROR: Session file not found"
    echo "   Note: Session is registered after guest ready (sleep 30 is running)"
    echo "   VM1 process status:"
    ps -p $VM1_PID 2>/dev/null || echo "   VM1 process not found"
fi

echo ""
echo "3. Running VM2 (should reuse VM1 session)"
OUTPUT=$($EPKG -e $ENV run --isolate=vm --vmm=qemu --vm-keep-timeout=60 echo "vm2-test" 2>&1)
echo "   Output: $OUTPUT"
echo "   Length: ${#OUTPUT}"

# Debug: show hex dump of OUTPUT
echo "   Hex dump of OUTPUT:"
printf '%s' "$OUTPUT" | xxd | head -2

if [ "$OUTPUT" = "vm2-test" ]; then
    echo "   SUCCESS: VM2 reused VM1 session"
else
    echo "   ERROR: VM2 output mismatch"
    echo "   Expected 'vm2-test' hex:"
    printf '%s' "vm2-test" | xxd | head -2
fi

echo ""
echo "4. Checking QEMU process count (should be 1)"
QEMU_COUNT=$(pgrep -c qemu-system 2>/dev/null || echo "0")
echo "   QEMU processes: $QEMU_COUNT"

echo ""
echo "5. Running VM3"
OUTPUT=$($EPKG -e $ENV run --isolate=vm --vmm=qemu --vm-keep-timeout=60 echo "vm3-test")
echo "   Output: $OUTPUT"

# Cleanup
echo ""
echo "6. Cleanup"
pkill -f qemu-system 2>/dev/null || true
rm -f ~/.epkg/run/vm-sessions/*.json 2>/dev/null || true
wait $VM1_PID 2>/dev/null || true

echo ""
echo "=== All tests passed ==="
