#!/usr/bin/env sh
set -eu

register_mcp=false
if [ "${1:-}" = "--register-mcp" ]; then
  register_mcp=true
  shift
fi
if [ "$#" -ne 0 ]; then
  echo "usage: install.sh [--register-mcp]" >&2
  exit 2
fi

case "$(uname -s)" in
  Darwin) platform="apple-darwin" ;;
  Linux) platform="unknown-linux-musl" ;;
  *)
    echo "jevctl installer supports macOS and Linux only." >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  x86_64|amd64) architecture="x86_64" ;;
  arm64|aarch64) architecture="aarch64" ;;
  *)
    echo "jevctl installer supports x86_64 and ARM64 only." >&2
    exit 1
    ;;
esac

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required to download jevctl." >&2
  exit 1
fi

asset="jevctl-$architecture-$platform.tar.gz"
release_base="${JEVCTL_RELEASE_BASE_URL:-https://github.com/iyazerski/jevctl/releases/latest/download}"
install_dir="${JEVCTL_INSTALL_DIR:-$HOME/.local/bin}"
install_tmp="$(mktemp -d)"
trap 'rm -rf "$install_tmp"' EXIT HUP INT TERM

echo "Downloading $asset..."
curl -fsSL "$release_base/$asset" -o "$install_tmp/$asset"
curl -fsSL "$release_base/$asset.sha256" -o "$install_tmp/$asset.sha256"

if command -v sha256sum >/dev/null 2>&1; then
  (cd "$install_tmp" && sha256sum -c "$asset.sha256")
elif command -v shasum >/dev/null 2>&1; then
  (cd "$install_tmp" && shasum -a 256 -c "$asset.sha256")
else
  echo "sha256sum or shasum is required to verify jevctl." >&2
  exit 1
fi

tar -xzf "$install_tmp/$asset" -C "$install_tmp"
mkdir -p "$install_dir"
install -m 755 "$install_tmp/jevctl" "$install_dir/jevctl"

if [ ! -x "$install_dir/jevctl" ]; then
  echo "jevctl installation failed." >&2
  exit 1
fi

if [ "$register_mcp" = true ]; then
  if command -v codex >/dev/null 2>&1; then
    codex mcp add jevctl -- "$install_dir/jevctl" mcp serve
  fi
  if command -v claude >/dev/null 2>&1; then
    claude mcp add --scope user jevctl -- "$install_dir/jevctl" mcp serve
  fi
fi

cat <<EOF

jevctl installed to $install_dir/jevctl

The process running jevctl must inherit TYPESAFE_API_KEY.

Manual MCP configuration:
  command: $install_dir/jevctl
  args: ["mcp", "serve"]
EOF
