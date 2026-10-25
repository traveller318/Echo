#!/usr/bin/env bash
# Installs Echo on Linux from a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/traveller318/Echo/main/scripts/install.sh | bash
#   ./install.sh --version v0.1.0 --yes
#
# It picks the package for your distribution (.deb on Debian/Ubuntu and relatives, .rpm on Fedora/RHEL/openSUSE and
# relatives, otherwise the AppImage in your home folder), downloads it together with the release's SHA256SUMS, checks
# the digest, and installs it. Nothing is installed if the digest does not match. The only privileged step is your
# package manager's own install (sudo); the AppImage path needs no privileges at all.
#
# Options:
#   --version TAG   install release TAG (default: the latest release)
#   --appimage      use the AppImage even on a distribution with packages
#   --yes           do not ask before installing
set -euo pipefail

REPO="${ECHO_REPO:-traveller318/Echo}"
VERSION=""
FORCE_APPIMAGE=0
ASSUME_YES=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) VERSION="${2:?--version needs a tag}"; shift 2 ;;
    --appimage) FORCE_APPIMAGE=1; shift ;;
    --yes | -y) ASSUME_YES=1; shift ;;
    -h | --help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "'$1' is required but not installed"; }

[[ "$(uname -s)" == "Linux" ]] || die "this installer is for Linux"
[[ "$(uname -m)" == "x86_64" ]] || die "only x86_64 builds are published (this machine is $(uname -m))"
need curl
need sha256sum

# Which kind of package fits this system?
kind="appimage"
if [[ "${FORCE_APPIMAGE}" -eq 0 && -r /etc/os-release ]]; then
  # shellcheck disable=SC1091
  . /etc/os-release
  family=" ${ID:-} ${ID_LIKE:-} "
  if [[ "${family}" == *" debian "* || "${family}" == *" ubuntu "* ]] && command -v apt-get >/dev/null 2>&1; then
    kind="deb"
  elif [[ "${family}" == *" fedora "* || "${family}" == *" rhel "* ]] && command -v dnf >/dev/null 2>&1; then
    kind="rpm"
  elif [[ "${family}" == *" suse "* || "${family}" == *" opensuse "* ]] && command -v zypper >/dev/null 2>&1; then
    kind="rpm"
  fi
fi

# Which release?
api="https://api.github.com/repos/${REPO}/releases"
if [[ -n "${VERSION}" ]]; then api="${api}/tags/${VERSION}"; else api="${api}/latest"; fi
say "Looking up the release (${VERSION:-latest}) of ${REPO}"
release="$(curl -fsSL -H 'Accept: application/vnd.github+json' "${api}")" || die "no such release: ${VERSION:-latest}"

# Release asset URLs, one per line.
assets="$(printf '%s' "${release}" | grep -o '"browser_download_url": *"[^"]*"' | sed 's/.*"\(https[^"]*\)"/\1/')"
pick() { printf '%s\n' "${assets}" | grep -E "$1" | head -n1 || true; }
case "${kind}" in
  deb) url="$(pick '_amd64\.deb$')" ;;
  rpm) url="$(pick '\.x86_64\.rpm$')" ;;
  appimage) url="$(pick '_amd64\.AppImage$')" ;;
esac
[[ -n "${url:-}" ]] || die "the release has no ${kind} package for x86_64"
sums_url="$(pick '(^|/)SHA256SUMS$')"
[[ -n "${sums_url}" ]] || die "the release has no SHA256SUMS file, so the download cannot be verified"
file="${url##*/}"

if [[ "${ASSUME_YES}" -eq 0 ]]; then
  say "About to install ${file} (${kind}). Continue? [y/N]"
  read -r answer </dev/tty || answer=""
  [[ "${answer}" =~ ^[Yy] ]] || die "cancelled"
fi

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
say "Downloading ${file}"
curl -fL --progress-bar -o "${work}/${file}" "${url}"
curl -fsSL -o "${work}/SHA256SUMS" "${sums_url}"

say "Verifying the checksum"
expected="$(grep -E "[ *]${file}\$" "${work}/SHA256SUMS" | head -n1 | cut -d' ' -f1 || true)"
[[ -n "${expected}" ]] || die "SHA256SUMS has no entry for ${file}"
actual="$(sha256sum "${work}/${file}" | cut -d' ' -f1)"
[[ "${expected}" == "${actual}" ]] || die "checksum mismatch for ${file} (expected ${expected}, got ${actual}); nothing was installed"

as_root() { if [[ "$(id -u)" -eq 0 ]]; then "$@"; else need sudo; sudo "$@"; fi; }

case "${kind}" in
  deb)
    say "Installing with apt (sudo)"
    as_root apt-get install -y "${work}/${file}"
    ;;
  rpm)
    if command -v dnf >/dev/null 2>&1; then
      say "Installing with dnf (sudo)"
      as_root dnf install -y "${work}/${file}"
    else
      say "Installing with zypper (sudo)"
      as_root zypper --non-interactive install --allow-unsigned-rpm "${work}/${file}"
    fi
    ;;
  appimage)
    say "Installing the AppImage into your home folder"
    share="${XDG_DATA_HOME:-$HOME/.local/share}"
    bin="$HOME/.local/bin"
    mkdir -p "${share}/echo" "${share}/applications" "${bin}"
    install -m 0755 "${work}/${file}" "${share}/echo/Echo.AppImage"
    ln -sf "${share}/echo/Echo.AppImage" "${bin}/echo-dictation"
    cat > "${share}/applications/echo-dictation.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Echo
Comment=Local, private speech-to-text
Exec="${share}/echo/Echo.AppImage"
Terminal=false
Categories=AudioVideo;Utility;
DESKTOP
    case ":${PATH}:" in *":${bin}:"*) ;; *) say "Add ${bin} to your PATH to run 'echo-dictation' from a terminal" ;; esac
    ;;
esac

say "Echo is installed. Start it from your application menu, or run: echo-dictation"
session="${XDG_SESSION_TYPE:-unknown}"
cat <<NOTES

First run:
  - A short setup picks your microphone and downloads the speech model (about 650 MB).
  - Session type: ${session}.
NOTES
if [[ "${session}" == "wayland" ]]; then
  cat <<'NOTES'
  - On Wayland your desktop asks you to approve Echo's keyboard shortcut (and, on GNOME and KDE, permission to paste).
    The default dictation shortcut is Ctrl+Alt+Space.
  - Hyprland / Sway / river and other desktops without a shortcut portal: bind a key to `echo-dictation --toggle`.
    Hyprland also needs a `global` bind named after the shortcut, e.g. app.echo.desktop:record (see the README).
  - GNOME shows tray icons only with the "AppIndicator" extension installed.
NOTES
fi
