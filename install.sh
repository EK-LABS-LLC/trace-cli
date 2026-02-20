#!/bin/sh
set -e

REPO="EK-LABS-LLC/trace-cli"
BINARY_NAME="pulse"
INSTALL_DIR="${PULSE_INSTALL_DIR:-$HOME/.local/bin}"

# --- helpers ---

info() {
  printf '  \033[1;34m>\033[0m %s\n' "$1"
}

ok() {
  printf '  \033[1;32m✓\033[0m %s\n' "$1"
}

err() {
  printf '  \033[1;31m✗\033[0m %s\n' "$1" >&2
  exit 1
}

# --- detect platform ---

detect_os() {
  case "$(uname -s)" in
    Linux*)  echo "linux" ;;
    Darwin*) echo "darwin" ;;
    *)       err "Unsupported OS: $(uname -s). Pulse supports Linux and macOS." ;;
  esac
}

detect_arch() {
  case "$(uname -m)" in
    x86_64|amd64)  echo "amd64" ;;
    aarch64|arm64)  echo "arm64" ;;
    *)              err "Unsupported architecture: $(uname -m). Pulse supports x86_64 and arm64." ;;
  esac
}

# --- resolve version ---

resolve_version() {
  if [ -n "$PULSE_VERSION" ]; then
    echo "$PULSE_VERSION"
    return
  fi

  local latest
  latest=$(curl -fsSL -H "Accept: application/vnd.github.v3+json" \
    "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
    | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

  if [ -z "$latest" ]; then
    err "Could not determine latest version. Set PULSE_VERSION=vX.Y.Z to install a specific version."
  fi

  echo "$latest"
}

# --- main ---

main() {
  printf '\n\033[1mpulse installer\033[0m\n\n'

  OS=$(detect_os)
  ARCH=$(detect_arch)
  VERSION=$(resolve_version)
  ARTIFACT="${BINARY_NAME}-${OS}-${ARCH}"

  info "Version:  ${VERSION}"
  info "Platform: ${OS}/${ARCH}"
  info "Target:   ${INSTALL_DIR}/${BINARY_NAME}"
  echo ""

  # download
  URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARTIFACT}.tar.gz"
  info "Downloading ${URL}"

  TMPDIR=$(mktemp -d)
  trap 'rm -rf "$TMPDIR"' EXIT

  if ! curl -fsSL "$URL" -o "${TMPDIR}/${ARTIFACT}.tar.gz"; then
    err "Download failed. Check that version ${VERSION} exists and has a ${OS}/${ARCH} binary."
  fi

  # extract
  tar xzf "${TMPDIR}/${ARTIFACT}.tar.gz" -C "$TMPDIR"

  # install
  mkdir -p "$INSTALL_DIR"
  mv "${TMPDIR}/${ARTIFACT}" "${INSTALL_DIR}/${BINARY_NAME}"
  chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

  ok "Installed pulse ${VERSION} to ${INSTALL_DIR}/${BINARY_NAME}"
  echo ""

  # check PATH
  case ":$PATH:" in
    *":${INSTALL_DIR}:"*) ;;
    *)
      printf '  \033[1;33m!\033[0m %s is not in your PATH. Add it:\n' "$INSTALL_DIR"
      echo ""
      echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
      echo ""
      echo "  Add that line to your ~/.bashrc or ~/.zshrc to make it permanent."
      echo ""
      ;;
  esac

  # quick start
  printf '  \033[1mGet started:\033[0m\n'
  echo ""
  echo "    pulse init          # configure trace service"
  echo "    pulse connect       # install hooks into your agents"
  echo "    pulse status        # verify setup"
  echo ""
}

main
