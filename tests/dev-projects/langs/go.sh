#!/bin/sh
# Minimal Go project: build and run a tiny program.

. "$(dirname "$0")/../common.sh"

# Install one of go/golang/gcc-go (Alpine has conflict between go and gcc-go if both requested)
# brew: need bash/coreutils for shell commands
if [ "$OS" = "brew" ]; then
    run_install go ca-certificates bash coreutils
else
    run_install go ca-certificates
fi
check_cmd go version || { run_install golang ca-certificates; check_cmd go version || { run_install gcc-go ca-certificates; check_cmd go version || lang_skip "no go package for OS=$OS"; }; }

run_ebin_if go version

# Check if running in VM mode (macOS/Windows running Linux distro)
# virtiofs in libkrun VM doesn't support poll, causing Go build to fail:
#   read /usr/lib/go/src/...: not pollable
_is_vm_mode() {
    local host_os
    host_os=$(uname -s)
    case "$OS" in
        conda|msys2|brew)
            return 1  # Native host distros, no VM
            ;;
    esac
    [ "$host_os" = "Darwin" ] && return 0  # macOS running Linux distro via VM
    return 1
}

# Use GOCACHE inside env so go build/run can write (avoid permission denied on host .cache)
# Create test file - use go for conda/Windows (no /bin/sh)
# brew: use bash instead of /bin/sh (vdso_time SIGSEGV)
if [ "$OS" = "conda" ]; then
    run go run -e -exec "" /dev/stdin <<'EOF'
package main
import (
    "fmt"
    "os"
)
func main() {
    os.MkdirAll("$TEST_TMP/goproj", 0755)
    f, _ := os.Create("$TEST_TMP/goproj/main.go")
    f.WriteString("package main\nimport \"fmt\"\nfunc main() { fmt.Println(\"ok\") }\n")
    f.Close()
    fmt.Println("created")
}
EOF
    run go build -o "$TEST_TMP/goproj/hello" "$TEST_TMP/goproj/main.go"
    run "$TEST_TMP/goproj/hello" | grep -q ok
    lang_ok
    exit 0
elif [ "$OS" = "brew" ]; then
    SHELL_CMD="bash -c"
else
    SHELL_CMD="/bin/sh -c"
fi

# VM mode: virtiofs doesn't support poll, Go build fails
# Skip build test, just verify go version works
if _is_vm_mode; then
    log "Skipping go build test in VM mode (virtiofs poll not supported)"
    lang_ok
    exit 0
fi

run $SHELL_CMD "mkdir -p $TEST_TMP/goproj && cd $TEST_TMP/goproj && printf '%s\n' 'package main' 'import \"fmt\"' 'func main() { fmt.Println(\"ok\") }' > main.go"
run $SHELL_CMD "export GOCACHE=$TEST_TMP/go-build && cd $TEST_TMP/goproj && go build -o hello main.go && ./hello"
# Run go get and go run in same shell to preserve $TEST_TMP/gogetproj between operations
run $SHELL_CMD "export GOCACHE=$TEST_TMP/go-build && cd $TEST_TMP && rm -rf gogetproj && mkdir -p gogetproj && cd gogetproj && go mod init test && go get rsc.io/quote && printf '%s\n' 'package main' 'import (' '\"fmt\"' '\"rsc.io/quote\"' ')' 'func main() { fmt.Println(quote.Hello()) }' > main.go && go run main.go"
lang_ok
