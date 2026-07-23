#!/bin/sh
set -eu

REPOSITORY="juanandresgs/NearManager"
BINARIES="near-fm near-view near-proc near-demo"

fail() {
    printf 'Near Manager install: %s\n' "$*" >&2
    exit 1
}

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar >/dev/null 2>&1 || fail "tar is required"

os_name=${NEAR_INSTALL_OS:-$(uname -s)}
case "$os_name" in
    Darwin|darwin|macos) platform=macos ;;
    Linux|linux) platform=linux ;;
    *) fail "unsupported operating system: $os_name" ;;
esac

arch_name=${NEAR_INSTALL_ARCH:-$(uname -m)}
case "$arch_name" in
    arm64|aarch64) architecture=aarch64 ;;
    x86_64|amd64) architecture=x86_64 ;;
    *) fail "unsupported CPU architecture: $arch_name" ;;
esac

if [ "$platform" = linux ] && [ "$architecture" != x86_64 ]; then
    fail "Linux releases currently support x86_64; detected $architecture"
fi

archive="near-${platform}-${architecture}.tar.gz"
if [ "${NEAR_INSTALL_DRY_RUN:-0}" = 1 ]; then
    printf '%s\n' "$archive"
    exit 0
fi

install_dir=${NEAR_INSTALL_DIR:-"${HOME:?HOME is required}/.local/bin"}
base_url=${NEAR_INSTALL_BASE_URL:-"https://github.com/${REPOSITORY}/releases/latest/download"}
case "$base_url" in
    https://*) curl_protocol="=https" ;;
    *)
        [ "${NEAR_INSTALL_ALLOW_INSECURE:-0}" = 1 ] || fail "release URL must use HTTPS"
        curl_protocol="=http,https,file"
        ;;
esac

temporary=$(mktemp -d "${TMPDIR:-/tmp}/near-manager-install.XXXXXX")
trap 'rm -rf "$temporary"' EXIT HUP INT TERM

printf 'Installing Near Manager for %s/%s...\n' "$platform" "$architecture"
curl --proto "$curl_protocol" --tlsv1.2 --fail --location --silent --show-error \
    "$base_url/$archive" --output "$temporary/$archive"
curl --proto "$curl_protocol" --tlsv1.2 --fail --location --silent --show-error \
    "$base_url/$archive.sha256" --output "$temporary/$archive.sha256"

(
    cd "$temporary"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum -c "$archive.sha256" >/dev/null
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 -c "$archive.sha256" >/dev/null
    else
        fail "sha256sum or shasum is required to verify the release"
    fi
)

tar -xzf "$temporary/$archive" -C "$temporary"
mkdir -p "$install_dir"
for binary in $BINARIES; do
    [ -f "$temporary/$binary" ] || fail "release archive is missing $binary"
    install -m 0755 "$temporary/$binary" "$install_dir/$binary"
done

case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *)
        shell_name=$(basename "${SHELL:-sh}")
        path_line='export PATH="$HOME/.local/bin:$PATH"'
        if [ "$install_dir" = "$HOME/.local/bin" ]; then
            add_path_line() {
                profile=$1
                line=$2
                mkdir -p "$(dirname "$profile")"
                if [ ! -f "$profile" ] || ! grep -F "$line" "$profile" >/dev/null 2>&1; then
                    printf '\n# Near Manager\n%s\n' "$line" >>"$profile"
                fi
                printf 'Added %s to PATH in %s.\n' "$install_dir" "$profile"
            }
            case "$shell_name" in
                zsh)
                    zsh_config=${ZDOTDIR:-$HOME}
                    add_path_line "$zsh_config/.zprofile" "$path_line"
                    add_path_line "$zsh_config/.zshrc" "$path_line"
                    ;;
                bash)
                    if [ -f "$HOME/.bash_profile" ]; then
                        bash_login=$HOME/.bash_profile
                    elif [ -f "$HOME/.bash_login" ]; then
                        bash_login=$HOME/.bash_login
                    else
                        bash_login=$HOME/.profile
                    fi
                    add_path_line "$bash_login" "$path_line"
                    add_path_line "$HOME/.bashrc" "$path_line"
                    ;;
                fish)
                    add_path_line "$HOME/.config/fish/config.fish" \
                        'fish_add_path "$HOME/.local/bin"'
                    ;;
                *) add_path_line "$HOME/.profile" "$path_line" ;;
            esac
        else
            printf 'Add %s to PATH to run Near Manager by name.\n' "$install_dir"
        fi
        ;;
esac

"$install_dir/near-fm" --version
printf 'Near Manager is installed. Open a new terminal and run: near-fm\n'
