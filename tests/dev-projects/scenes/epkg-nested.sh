#!/bin/sh
# Test nested epkg/apk/apt/dnf commands inside an epkg environment

. "$(dirname "$0")/../common.sh"

run_install bash

extract_core_fields() {
    sed 's/^[IU]*[[:space:]]*//' | awk '{print $1, $2, $3, $4, $5, $6}'
}

# Direct (host-side) vs nested (inside-env) epkg list comparison
list1=$("$EPKG_BIN" -e "$ENV_NAME" list | tail -n +5 | grep -v '^Total' | grep -v '^$' | extract_core_fields | sort)

case "$(uname -s)" in
    Darwin)
        # macOS (VM mode): EPKG_ACTIVE_ENV auto-detects env
        list2=$(run bash -c "epkg list" | tail -n +5 | grep -v '^Total' | grep -v '^$' | extract_core_fields | sort)
        ;;
    *)
        list2=$(run bash -c "epkg -e \"$ENV_NAME\" list" | tail -n +5 | grep -v '^Total' | grep -v '^$' | extract_core_fields | sort)
        ;;
esac

if [ "$list1" != "$list2" ]; then
    echo "ERROR: epkg list core fields differ between direct and nested" >&2
    # Use temp files for portable diff (process substitution <(...) is bash-only)
    tmp1=$(mktemp)
    tmp2=$(mktemp)
    echo "$list1" > "$tmp1"
    echo "$list2" > "$tmp2"
    diff -u "$tmp1" "$tmp2" >&2 || true
    rm -f "$tmp1" "$tmp2"
    exit 1
fi

# Test nested package management via epkg/apk/apt/dnf shims
for entry in "epkg:list:install:remove" \
             "apk:list:add:del" \
             "apt:list:install:remove" \
             "dnf:list:install:remove"; do
    cmd=$(echo "$entry"       | cut -d: -f1)
    list_subcmd=$(echo "$entry"    | cut -d: -f2)
    install_subcmd=$(echo "$entry" | cut -d: -f3)
    remove_subcmd=$(echo "$entry"  | cut -d: -f4)

    run bash -c "$cmd $list_subcmd"
    run bash -c "$cmd -y $install_subcmd tree"
    run bash -c "$cmd $list_subcmd" | grep -qw tree || exit 1
    run bash -c "$cmd -y $remove_subcmd tree"
done

lang_ok
