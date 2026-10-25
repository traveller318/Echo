#!/usr/bin/env bash
# Removes Echo from Linux: the package (or the AppImage install) and, with --purge, your recordings, history and
# downloaded models. Your settings and history stay unless you pass --purge.
#
#   ./uninstall.sh            remove the program only
#   ./uninstall.sh --purge    also delete ~/.local/share/app.echo.desktop (history, recordings, models)
set -euo pipefail

PURGE=0
[[ "${1:-}" == "--purge" ]] && PURGE=1

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
as_root() { if [[ "$(id -u)" -eq 0 ]]; then "$@"; else sudo "$@"; fi; }

share="${XDG_DATA_HOME:-$HOME/.local/share}"

if command -v dpkg >/dev/null 2>&1 && dpkg -s echo >/dev/null 2>&1; then
  say "Removing the deb package (sudo)"
  as_root apt-get remove -y echo
elif command -v rpm >/dev/null 2>&1 && rpm -q echo >/dev/null 2>&1; then
  say "Removing the rpm package (sudo)"
  if command -v dnf >/dev/null 2>&1; then as_root dnf remove -y echo; else as_root zypper --non-interactive remove echo; fi
fi

# The AppImage install, the start-at-sign-in entry and the portal identity entry Echo writes for itself.
rm -rf "${share}/echo"
rm -f "$HOME/.local/bin/echo-dictation" \
  "${share}/applications/echo-dictation.desktop" \
  "${share}/applications/app.echo.desktop.desktop" \
  "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/app.echo.desktop.desktop"

if [[ "${PURGE}" -eq 1 ]]; then
  say "Deleting ${share}/app.echo.desktop (history, recordings, models)"
  rm -rf "${share}/app.echo.desktop"
else
  say "Kept ${share}/app.echo.desktop (history, recordings, models); use --purge to delete it"
fi
say "Echo is removed."
