#!/bin/sh
# Minimal root: chroot with only epkg + busybox, then self install, env at --root /, install + run.

. "$(dirname "$0")/../vars.sh"
. "$(dirname "$0")/../lib.sh"

if [ "${E2E_BACKEND:-}" != vm ]; then
	log "Skipping bare rootfs test outside VM (needs chroot + tmpfs; run via test-one.sh / vm.sh)."
	exit 0
fi

BUSYBOX_BIN="${BUSYBOX_BIN:-$(command -v busybox)}"
if [ -z "$BUSYBOX_BIN" ] && [ -x /bin/busybox ]; then
	BUSYBOX_BIN=/bin/busybox
fi
if [ -z "$BUSYBOX_BIN" ] || [ ! -x "$BUSYBOX_BIN" ]; then
	error "busybox not found (set BUSYBOX_BIN)"
fi

CHROOT="${E2E_BARE_CHROOT:-/tmp/epkg-bare-chroot}"

cleanup() {
	rm -rf "$CHROOT"
}
trap cleanup EXIT INT HUP

log "Preparing chroot at $CHROOT"
mkdir -p "$CHROOT"
mkdir -p "$CHROOT"/bin "$CHROOT"/usr/bin "$CHROOT"/dev "$CHROOT"/proc "$CHROOT"/sys "$CHROOT"/etc "$CHROOT"/tmp

cp -a "$BUSYBOX_BIN" "$CHROOT/bin/busybox" || error "copy busybox"
chmod +x "$CHROOT/bin/busybox"
ln -sf busybox "$CHROOT/bin/sh"
# Also copy as busybox.static to avoid epkg's built-in busybox interception
cp -a "$BUSYBOX_BIN" "$CHROOT/bin/busybox.static" || true

cp -a "$EPKG_BINARY" "$CHROOT/usr/bin/epkg" || error "copy epkg"
chmod +x "$CHROOT/usr/bin/epkg"

# Bootstrap basic network identity files in chroot.
# vm.sh usually bind-mounts resolv.conf, but keep a fallback here for robustness.
if [ -s /etc/resolv.conf ]; then
	cp -a /etc/resolv.conf "$CHROOT/etc/resolv.conf" || true
fi
if ! grep -q '^nameserver[[:space:]]' "$CHROOT/etc/resolv.conf"; then
	{
		echo "nameserver 10.0.2.3"
		echo "nameserver 1.1.1.1"
		echo "nameserver 8.8.8.8"
	} >"$CHROOT/etc/resolv.conf"
fi
[ -s /etc/hosts ] && cp -a /etc/hosts "$CHROOT/etc/hosts" || true
log "chroot resolv.conf:"
sed -n '1,20p' "$CHROOT/etc/resolv.conf" || true

log "In-guest identity/caps before chroot pseudo-fs mounts"
id || true
grep -E '^(Uid|Gid|Groups|CapInh|CapPrm|CapEff|CapBnd|NoNewPrivs):' /proc/self/status || true

mount -t devtmpfs devtmpfs "$CHROOT/dev" || error "mount devtmpfs"
mount -t proc proc "$CHROOT/proc" || error "mount proc"
mount -t sysfs sysfs "$CHROOT/sys" || error "mount sysfs"

log "Network sanity check: curl bing.com"
# epkg intercepts busybox applet calls, so use wget/curl for network test instead
# Skip if not available - this is just a sanity check
if [ -x "$CHROOT/usr/bin/curl" ] || [ -x "$CHROOT/bin/curl" ]; then
	CURL_BIN=$(ls "$CHROOT"/usr/bin/curl "$CHROOT"/bin/curl 2>/dev/null | head -1)
	chroot "$CHROOT" "$CURL_BIN" -sI http://bing.com --connect-timeout 5 || log "Warning: curl test failed (network may be unavailable)"
elif [ -x "$CHROOT/usr/bin/wget" ] || [ -x "$CHROOT/bin/wget" ]; then
	WGET_BIN=$(ls "$CHROOT"/usr/bin/wget "$CHROOT"/bin/wget 2>/dev/null | head -1)
	chroot "$CHROOT" "$WGET_BIN" -q --spider http://bing.com || log "Warning: wget test failed (network may be unavailable)"
else
	log "Warning: no curl/wget available for network test, skipping"
fi

log "epkg self install inside chroot"
chroot "$CHROOT" /usr/bin/epkg self install || error "self install in chroot"

log "Creating sys env with --root / inside chroot"
chroot "$CHROOT" /usr/bin/epkg env create sys -c alpine --root / || error "env create sys --root /"

log "Installing jq in sys"
chroot "$CHROOT" /usr/bin/epkg -e sys --assume-yes install jq coreutils bash || error "install jq"

log "Verifying jq via epkg run (chroot)"
chroot "$CHROOT" /usr/bin/epkg -e sys run jq --version || error "jq run failed"

log "Removing sys environment"
chroot "$CHROOT" /usr/bin/epkg --assume-yes env remove sys || true

log "Bare rootfs (chroot) test completed successfully"
