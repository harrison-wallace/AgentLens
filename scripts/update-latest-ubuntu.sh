#!/usr/bin/env bash
#
# Install (or update to) the latest AgentLens release on Ubuntu.
#
#   scripts/update-latest-ubuntu.sh            # install the newest release
#   scripts/update-latest-ubuntu.sh --force    # reinstall even if up to date
#   scripts/update-latest-ubuntu.sh --tag v0.1.0
#
# Draft releases count: the gh CLI lists them for users who can see them, so
# this works before a release is published. Requires gh, authenticated.

set -euo pipefail

REPO="harrison-wallace/AgentLens"
PACKAGE="agent-lens" # deb package name (the binary is `agentlens`)
FORCE=0
TAG=""

usage() {
  cat <<'EOF'
Usage: scripts/update-latest-ubuntu.sh [options]

      --tag TAG    Install a specific tag instead of the newest release
      --force      Reinstall even if that version is already installed
  -h, --help       Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)
      TAG="${2:-}"
      shift 2
      ;;
    --force)
      FORCE=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

step() { printf '\n\033[1;34m==>\033[0m %s\n' "$1"; }
fail() {
  printf '\033[1;31merror:\033[0m %s\n' "$1" >&2
  exit 1
}

command -v gh >/dev/null || fail "the gh CLI is required (apt install gh, then gh auth login)"
gh auth status >/dev/null 2>&1 || fail "gh is not authenticated — run: gh auth login"

step "Finding the release"
if [[ -z "$TAG" ]]; then
  TAG="$(gh release list --repo "$REPO" --limit 1 --json tagName --jq '.[0].tagName // empty')"
  [[ -n "$TAG" ]] || fail "no releases found in $REPO"
fi
VERSION="${TAG#v}"
echo "latest release: $TAG"

INSTALLED="$(dpkg-query -W -f='${Version}' "$PACKAGE" 2>/dev/null || true)"
if [[ -n "$INSTALLED" ]]; then
  echo "installed: $INSTALLED"
else
  echo "installed: none"
fi

if [[ "$INSTALLED" == "$VERSION" && $FORCE -eq 0 ]]; then
  echo "already up to date — pass --force to reinstall"
  exit 0
fi

step "Downloading the .deb"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT
gh release download "$TAG" --repo "$REPO" --dir "$TMPDIR" --pattern '*.deb' ||
  fail "no .deb asset on $TAG (is the release build still running?)"

DEB="$(find "$TMPDIR" -maxdepth 1 -name '*.deb' -print -quit)"
[[ -n "$DEB" ]] || fail "downloaded no .deb"
echo "got $(basename "$DEB")"

step "Installing (sudo)"
sudo apt-get install -y --allow-downgrades "$DEB"

step "Done"
dpkg-query -W -f='${Package} ${Version}\n' "$PACKAGE" 2>/dev/null || true
echo "launch it with: agentlens"
