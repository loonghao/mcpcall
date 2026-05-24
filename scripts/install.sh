#!/usr/bin/env sh
set -eu

repo="${MCPCALL_REPO:-loonghao/mcpcall}"
version="${MCPCALL_VERSION:-latest}"
install_dir="${MCPCALL_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)" in
  Linux)
    case "$(uname -m)" in
      x86_64|amd64)
        artifact="mcpcall-linux-x86_64"
        ;;
      aarch64|arm64)
        artifact="mcpcall-linux-aarch64"
        ;;
      *)
        echo "unsupported Linux architecture: $(uname -m)" >&2
        exit 1
        ;;
    esac
    binary="mcpcall"
    ;;
  Darwin)
    case "$(uname -m)" in
      x86_64|amd64)
        artifact="mcpcall-macos-x86_64"
        ;;
      aarch64|arm64)
        artifact="mcpcall-macos-aarch64"
        ;;
      *)
        echo "unsupported macOS architecture: $(uname -m)" >&2
        exit 1
        ;;
    esac
    binary="mcpcall"
    ;;
  *)
    echo "unsupported OS: $(uname -s)" >&2
    exit 1
    ;;
esac

mkdir -p "$install_dir"

if [ "$version" = "latest" ]; then
  url="https://github.com/$repo/releases/latest/download/$artifact"
else
  url="https://github.com/$repo/releases/download/$version/$artifact"
fi

tmp_file="$(mktemp)"
cleanup() {
  rm -f "$tmp_file"
}
trap cleanup EXIT

auth_header=""
if [ -n "${GITHUB_TOKEN:-}" ]; then
  auth_header="Authorization: Bearer $GITHUB_TOKEN"
fi

if command -v curl >/dev/null 2>&1; then
  if [ -n "$auth_header" ]; then
    curl -fsSL -H "$auth_header" "$url" -o "$tmp_file"
  else
    curl -fsSL "$url" -o "$tmp_file"
  fi
elif command -v wget >/dev/null 2>&1; then
  if [ -n "$auth_header" ]; then
    wget --header="$auth_header" -qO "$tmp_file" "$url"
  else
    wget -qO "$tmp_file" "$url"
  fi
else
  echo "curl or wget is required" >&2
  exit 1
fi

target="$install_dir/$binary"
mv "$tmp_file" "$target"
chmod +x "$target"

if [ -n "${GITHUB_PATH:-}" ]; then
  echo "$install_dir" >> "$GITHUB_PATH"
fi

"$target" --version
echo "installed $target"
