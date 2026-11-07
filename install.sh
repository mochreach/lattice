#!/bin/sh
# Installs Lattice on Linux or macOS from its latest GitHub release:
#
#     curl -fsSL https://raw.githubusercontent.com/mochreach/lattice/main/install.sh | sh
#
# It picks the build for this computer, checks its checksum, puts `lattice` in ~/.local/bin and
# the example songs in ~/.local/share/lattice/examples, and says what else is needed. Nothing
# outside those two folders is changed, and nothing needs sudo. Settings, all optional:
#
#     LATTICE_VERSION       a release tag, such as v0.1.0, rather than the latest
#     LATTICE_INSTALL_DIR   where `lattice` goes, rather than ~/.local/bin
#     LATTICE_SHARE_DIR     where the examples go, rather than ~/.local/share/lattice
#     LATTICE_BUILD         a build by name, such as x86_64-linux, rather than this computer's
#
# Running it again updates Lattice.

# All in a function, run on the last line, so a download cut short runs nothing.
main() {
    set -eu
    repo=mochreach/lattice
    base=${LATTICE_BASE_URL:-https://github.com/$repo/releases}
    version=${LATTICE_VERSION:-latest}
    bin=${LATTICE_INSTALL_DIR:-$HOME/.local/bin}
    share=${LATTICE_SHARE_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/lattice}

    build=${LATTICE_BUILD:-$(pick_build)}
    if [ "$version" = latest ]; then
        url=$base/latest/download/lattice-$build.tar.gz
    else
        url=$base/download/$version/lattice-$build.tar.gz
    fi

    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "Downloading Lattice ($build)…"
    fetch "$url" "$tmp/lattice.tar.gz"
    fetch "$url.sha256" "$tmp/lattice.tar.gz.sha256"
    check "$tmp/lattice.tar.gz" "$tmp/lattice.tar.gz.sha256"
    tar -xzf "$tmp/lattice.tar.gz" -C "$tmp"
    unpacked=$tmp/lattice-$build

    mkdir -p "$bin" "$share"
    # Replaced rather than written over, so a Lattice that's running carries on undisturbed.
    cp "$unpacked/lattice" "$bin/lattice.new"
    chmod +x "$bin/lattice.new"
    mv -f "$bin/lattice.new" "$bin/lattice"
    rm -rf "$share/examples"
    cp -R "$unpacked/examples" "$share/examples"

    say ""
    say "Installed $("$bin/lattice" --version 2>/dev/null || echo lattice) in $bin/lattice"
    say "The example songs are in $share/examples"
    after "$bin" "$share"
}

say() {
    printf '%s\n' "$1"
}

fail() {
    printf 'lattice install: %s\n' "$1" >&2
    exit 1
}

# The build for this computer, by the names the releases use.
pick_build() {
    os=$(uname -s)
    arch=$(uname -m)
    case "$os" in
        Linux)
            [ -e /etc/NIXOS ] && fail "on NixOS, build it with Nix instead: nix run github:mochreach/lattice"
            case "$arch" in
                x86_64 | amd64)
                    # x86-64-v3: AVX2 and the rest that came with it, which nearly every x86_64
                    # since 2015 has.
                    if v3; then echo x86_64-linux-v3; else echo x86_64-linux; fi
                    ;;
                aarch64 | arm64) echo aarch64-linux ;;
                armv7l | armv6l) fail "there's no 32-bit ARM build: on a Raspberry Pi, use a 64-bit OS" ;;
                *) fail "there's no build for $arch Linux: see the README for building it yourself" ;;
            esac
            ;;
        Darwin)
            # A shell under Rosetta says x86_64 on an Apple Silicon Mac, which wants the arm64 build.
            if [ "$arch" = arm64 ] || [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = 1 ]; then
                echo aarch64-macos
            else
                echo x86_64-macos
            fi
            ;;
        MINGW* | MSYS* | CYGWIN*)
            fail "on Windows, run this in PowerShell instead: irm https://raw.githubusercontent.com/$repo/main/install.ps1 | iex"
            ;;
        *) fail "there's no build for $os: see the README for building it yourself" ;;
    esac
}

# Whether this CPU has everything x86-64-v3 asks for.
v3() {
    [ -r /proc/cpuinfo ] || return 1
    flags=$(grep -m1 '^flags' /proc/cpuinfo)
    for flag in avx2 bmi1 bmi2 fma f16c movbe abm; do
        case " $flags " in
            *" $flag "*) ;;
            *) return 1 ;;
        esac
    done
}

fetch() {
    if command -v curl > /dev/null; then
        curl -fsSL "$1" -o "$2" || fail "couldn't download $1"
    elif command -v wget > /dev/null; then
        wget -qO "$2" "$1" || fail "couldn't download $1"
    else
        fail "this needs curl or wget to download Lattice"
    fi
}

# Checks the archive against the checksum published beside it.
check() {
    want=$(cut -d ' ' -f 1 < "$2")
    if command -v sha256sum > /dev/null; then
        got=$(sha256sum "$1" | cut -d ' ' -f 1)
    elif command -v shasum > /dev/null; then
        got=$(shasum -a 256 "$1" | cut -d ' ' -f 1)
    else
        say "(no sha256sum or shasum here, so the download isn't checked)"
        return
    fi
    [ "$want" = "$got" ] || fail "the download doesn't match its checksum: try again"
}

# What else is needed, for this system.
after() {
    bin=$1
    share=$2
    case ":$PATH:" in
        *":$bin:"*) ;;
        *)
            say ""
            say "$bin isn't on your PATH. Add it with this line in your shell's startup file"
            case "${SHELL:-}" in
                */fish) say "(~/.config/fish/config.fish):"; say "    fish_add_path $bin" ;;
                */zsh) say "(~/.zshrc):"; say "    export PATH=\"$bin:\$PATH\"" ;;
                *) say "(~/.bashrc, or ~/.profile):"; say "    export PATH=\"$bin:\$PATH\"" ;;
            esac
            ;;
    esac
    if [ "$(uname -s)" = Linux ] && ! has_alsa; then
        say ""
        say "Lattice needs ALSA's library (libasound.so.2) to play, and it isn't installed:"
        say "    Debian, Ubuntu:  sudo apt install libasound2t64   (or libasound2 on older ones)"
        say "    Fedora:          sudo dnf install alsa-lib"
        say "    Arch:            sudo pacman -S alsa-lib"
    fi
    say ""
    say "Lattice wants a terminal that reports every key, as kitty, WezTerm (with"
    say "enable_kitty_keyboard = true), Ghostty, foot and Alacritty do."
    if [ "$(uname -s)" = Darwin ]; then
        say "On a Mac, use one of those rather than Terminal.app, and set Option to act as Alt:"
        say "macos_option_as_alt yes in kitty, macos-option-as-alt = true in Ghostty."
    fi
    say ""
    say "Start with the tutor:     lattice --tutor"
    say "Or open an example:       lattice $share/examples/amen/amen.lattice"
    say "If you're stuck:          lattice --help"
}

has_alsa() {
    if command -v ldconfig > /dev/null; then
        ldconfig -p 2> /dev/null | grep -q 'libasound\.so\.2' && return 0
    fi
    for dir in /usr/lib /usr/lib64 /lib /lib64 /usr/lib/x86_64-linux-gnu /usr/lib/aarch64-linux-gnu; do
        [ -e "$dir/libasound.so.2" ] && return 0
    done
    return 1
}

main "$@"
