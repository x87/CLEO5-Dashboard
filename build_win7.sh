#!/usr/bin/env bash
#
# Build SA.Dashboard.cleo for Windows 7 (i686-win7-windows-msvc, tier 3).
#
# The win7 std has to be compiled from source (`-Z build-std`): rustup ships no
# prebuilt std for tier-3 targets, and it is that std where ProcessPrng / WaitOnAddress /
# GetSystemTimePreciseAsFileTime are replaced with Win7-safe fallbacks.
#
# See docs/WIN7_BUILD.md in the hudhook checkout for the reasoning behind every flag.
# Override the toolchain with WIN7_TOOLCHAIN=nightly-<date>.

set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")"

TARGET=i686-win7-windows-msvc
OUT=SA.Dashboard.cleo
PINNED_TOOLCHAIN=nightly-2026-07-30

# Win8/Win10-only imports, plus the UCRT/VCRUNTIME redist a static CRT must not need.
FORBIDDEN='WaitOnAddress|WakeByAddress|ProcessPrng|PreciseAsFileTime|api-ms-win-core-synch|bcryptprimitives|api-ms-win-crt|VCRUNTIME14'

to_win_path() {
    if command -v cygpath >/dev/null 2>&1; then
        cygpath -w -- "$1"
    else
        printf '%s\n' "$1"
    fi
}

find_dumpbin() {
    command -v dumpbin 2>/dev/null && return 0

    local pf86 vswhere
    pf86=$(printenv 'ProgramFiles(x86)' || true)
    vswhere="${pf86:-C:\\Program Files (x86)}\\Microsoft Visual Studio\\Installer\\vswhere.exe"
    [[ -f $vswhere ]] || return 1

    "$vswhere" -latest -products '*' \
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 \
        -find 'VC\Tools\MSVC\**\bin\Hostx64\x86\dumpbin.exe' 2>/dev/null | head -n 1
}

# ---------------------------------------------------------------- toolchain

if [[ -n ${WIN7_TOOLCHAIN:-} ]]; then
    TC=$WIN7_TOOLCHAIN
elif rustup toolchain list | grep -q "^${PINNED_TOOLCHAIN}"; then
    TC=$PINNED_TOOLCHAIN
else
    # Any nightly >= 1.85 works; the floating one is just the least surprising default.
    TC=nightly
fi

if ! rustup component list --toolchain "$TC" --installed 2>/dev/null | grep -q '^rust-src'; then
    echo "installing rust-src for $TC (needed by -Z build-std)"
    rustup component add rust-src --toolchain "$TC"
fi

echo "toolchain: $(rustc +"$TC" -vV | head -n 1)"

# ------------------------------------------------------- linker workaround

# windows_i686_msvc 0.42.2 only advertises its lib dir for the pc/uwp triples, so the win7
# target cannot find windows.lib without pointing the linker at it (chain: parking_lot_core
# 0.9 -> windows-sys 0.45 -> windows-targets 0.42.2). Drop all of this once parking_lot
# reaches >= 0.12.2.
CARGO_ROOT="${CARGO_HOME:-$HOME/.cargo}"

find_windows_lib() {
    local dir
    for dir in "$CARGO_ROOT"/registry/src/*/windows_i686_msvc-0.42.2/lib; do
        if [[ -d $dir ]]; then
            printf '%s\n' "$dir"
            return 0
        fi
    done

    # Cargo only unpacks a crate when it builds it, so take the directory out of the cache.
    cargo fetch >/dev/null 2>&1 || true
    local crate
    crate=$(ls "$CARGO_ROOT"/registry/cache/*/windows_i686_msvc-0.42.2.crate 2>/dev/null | head -n 1)
    [[ -n $crate ]] || return 1

    rm -rf .tmp/winlib
    mkdir -p .tmp/winlib
    tar -xzf "$crate" -C .tmp/winlib || return 1
    printf '%s\n' "$PWD/.tmp/winlib/windows_i686_msvc-0.42.2/lib"
}

FLAGS=(-C target-feature=+crt-static -C "link-arg=/OUT:$OUT" -L SDK)
if LIB_DIR=$(find_windows_lib); then
    WIN_LIB=$(to_win_path "$LIB_DIR")
    case $WIN_LIB in
        *' '*) echo "warning: '$WIN_LIB' has a space; link.exe will not find windows.lib" >&2 ;;
    esac
    FLAGS+=(-L "native=$WIN_LIB")
else
    echo "warning: windows_i686_msvc-0.42.2 is not in the cargo registry; if the link fails on" >&2
    echo "         'cannot open input file windows.lib', check the parking_lot version in" >&2
    echo "         Cargo.lock (>= 0.12.2 makes this workaround unnecessary)." >&2
fi

# .cargo/config.toml rustflags are keyed to i686-pc-windows-msvc, so they do not apply here.
export RUSTFLAGS="${FLAGS[*]}"

# -------------------------------------------------------------------- build

echo "building $TARGET -> $OUT"
if ! cargo +"$TC" -Z build-std=std build --release --target "$TARGET"; then
    cat >&2 <<'EOF'

Build failed. Common causes:
  LNK1104 ... SA.Dashboard.cleo   the game is still running and holds the file
  LNK1181 ... windows.lib         see the windows_i686_msvc warning above
  E0463 can't find crate 'std'    -Z build-std or rust-src missing
  'Z' is only accepted on nightly wrong toolchain; pass WIN7_TOOLCHAIN=nightly-<date>
EOF
    exit 1
fi

# ---------------------------------------------------------------- verify

if [[ ! -s $OUT ]]; then
    echo "error: $OUT was not produced (is /OUT: still in RUSTFLAGS?)" >&2
    exit 1
fi

echo
echo "artifact: $OUT ($(wc -c <"$OUT" | tr -d ' ') bytes)"

DUMPBIN=$(find_dumpbin || true)
if [[ -z $DUMPBIN ]]; then
    echo "dumpbin not found; verify the import table by hand:"
    echo "  dumpbin /imports $OUT | findstr /i \"WaitOnAddress ProcessPrng api-ms-win-crt\""
    exit 0
fi

HITS=$("$DUMPBIN" /imports "$(to_win_path "$OUT")" | grep -Ei "$FORBIDDEN" || true)
if [[ -n $HITS ]]; then
    echo "error: Win8+/UCRT imports found - this binary is not Win7-safe:" >&2
    echo "$HITS" >&2
    exit 1
fi

echo "imports: ok (no Win8+ / UCRT dependencies)"
echo
echo "Deploy: close the game, then copy $OUT into <GTA San Andreas>\\cleo\\cleo_plugins\\"
