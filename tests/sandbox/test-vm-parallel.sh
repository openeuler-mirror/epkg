#!/bin/bash
# Test parallel VM clients with dynamic CID allocation
# Tests both QEMU and libkrun backends

set -e

EPKG=/c/epkg/target/debug/epkg
ENV=alpine

test_backend() {
    BACKEND=$1
    echo ""
    echo "=== Testing $BACKEND backend ==="

    # Clean up
    $EPKG vm stop $ENV 2>/dev/null || true
    rm -f ~/.epkg/run/vm-sessions/*.json 2>/dev/null || true
    rm -f ~/.epkg/run/vsock-*.sock 2>/dev/null || true
    rm -f ~/.epkg/run/ready-*.sock 2>/dev/null || true
    sleep 1

    echo ""
    echo "1. Starting VM1 (with vm-keep-timeout for session reuse)"
    # sleep 3 completes quickly, session stays active via vm-keep-timeout
    $EPKG -e $ENV run --isolate=vm --vmm=$BACKEND --vm-keep-timeout=15 sleep 3 &
    VM1_PID=$!
    echo "   VM1 PID: $VM1_PID"

    # Wait for VM1 to start, run sleep 3, and register session
    # Session is registered after guest ready (before sleep 3 starts)
    # After sleep 3 completes, VM stays alive for vm-keep-timeout seconds
    sleep 5

    echo ""
    echo "2. Checking session file"
    if [ -f ~/.epkg/run/vm-sessions/$ENV.json ]; then
        echo "   Session exists:"
        cat ~/.epkg/run/vm-sessions/$ENV.json | head -5
        SOCKET=$(grep socket_path ~/.epkg/run/vm-sessions/$ENV.json | sed 's/.*"socket_path": "//' | tr -d '"')
        echo "   Socket: $SOCKET"
    else
        echo "   ERROR: Session file not found"
        return 1
    fi

    echo ""
    echo "3. Running VM2 (should reuse VM1 session)"
    OUTPUT=$($EPKG -e $ENV run --isolate=vm --vmm=$BACKEND --vm-keep-timeout=60 echo "vm2-test" 2>&1)
    echo "   Output: '$OUTPUT'"

    if [ "$OUTPUT" = "vm2-test" ]; then
        echo "   SUCCESS: VM2 reused VM1 session"
    else
        echo "   ERROR: VM2 output mismatch"
        echo "   Length: ${#OUTPUT}, hex:"
        printf '%s' "$OUTPUT" | xxd | head -2
        return 1
    fi

    echo ""
    echo "4. Checking VM process count (should be 1)"
    if [ "$BACKEND" = "qemu" ]; then
        VM_COUNT=$(pgrep -c qemu-system 2>/dev/null || echo "0")
    else
        # libkrun process is 'epkg' running VM
        VM_COUNT=$(pgrep -c -f "epkg.*--isolate=vm.*--vmm=libkrun" 2>/dev/null || echo "0")
    fi
    echo "   $BACKEND processes: $VM_COUNT"

    echo ""
    echo "5. Running VM3"
    OUTPUT3=$($EPKG -e $ENV run --isolate=vm --vmm=$BACKEND --vm-keep-timeout=60 echo "vm3-test" 2>&1)
    echo "   Output: '$OUTPUT3'"

    if [ "$OUTPUT3" = "vm3-test" ]; then
        echo "   SUCCESS: VM3 reused VM1 session"
    else
        echo "   ERROR: VM3 output mismatch"
        return 1
    fi

    # Cleanup - let VM exit naturally after vm-keep-timeout expires
    echo ""
    echo "6. Cleanup"
    # Wait for VM to exit naturally (vm-keep-timeout handles shutdown)
    # No need to force stop - the session will expire after timeout
    rm -f ~/.epkg/run/vm-sessions/*.json 2>/dev/null || true
    wait $VM1_PID 2>/dev/null || true
    echo "   VM exited"

    echo ""
    echo "=== $BACKEND tests passed ==="
}

echo "=== Test Parallel VM Clients ==="

# Test QEMU backend
test_backend qemu

# Test libkrun backend (auto-enabled for x86_64 linux)
echo ""
echo "=== Testing libkrun backend ==="
# libkrun is auto-enabled for x86_64 linux in Makefile
# Just try to use it - will fail gracefully if not available
if [ "$(uname -m)" = "x86_64" ] && [ "$(uname -s)" = "Linux" ]; then
    test_backend libkrun
else
    echo "   libkrun not supported on this platform, skipping"
fi

echo ""
echo "=== All tests passed ==="