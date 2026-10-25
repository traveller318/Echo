#!/usr/bin/env bash
# Fetches the pinned ONNX Runtime CPU build for Linux x64 into src-tauri/resources/onnxruntime/.
#
# The Windows bundle is committed; the Linux library is 22 MB, so it is downloaded instead and checked against the
# SHA-256 of the release archive below before anything is unpacked. Run it once before `pnpm tauri dev|build` on Linux
# (CI and install-from-source do). Version 1.24.x is what the pinned `ort` crate (API 24) expects.
set -euo pipefail

VERSION="1.24.2"
SHA256="43725474ba5663642e17684717946693850e2005efbd724ac72da278fead25e6"
URL="https://github.com/microsoft/onnxruntime/releases/download/v${VERSION}/onnxruntime-linux-x64-${VERSION}.tgz"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dest="${root}/src-tauri/resources/onnxruntime"
licenses="${root}/src-tauri/resources/licenses"
marker="${root}/src-tauri/resources/.onnxruntime-linux-version"

if [[ "$(uname -m)" != "x86_64" ]]; then
  echo "error: only x86_64 Linux is supported (this machine is $(uname -m))" >&2
  exit 1
fi

if [[ -f "${dest}/libonnxruntime.so" && -f "${marker}" && "$(cat "${marker}")" == "${VERSION}" ]]; then
  echo "ONNX Runtime ${VERSION} is already in place."
  exit 0
fi

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

echo "Downloading ONNX Runtime ${VERSION} ..."
curl --fail --silent --show-error --location --output "${work}/ort.tgz" "${URL}"

actual="$(sha256sum "${work}/ort.tgz" | cut -d' ' -f1)"
if [[ "${actual}" != "${SHA256}" ]]; then
  echo "error: checksum mismatch for ${URL}" >&2
  echo "  expected ${SHA256}" >&2
  echo "  got      ${actual}" >&2
  exit 1
fi

tar --extract --gzip --file "${work}/ort.tgz" --directory "${work}"
src="${work}/onnxruntime-linux-x64-${VERSION}"

mkdir -p "${dest}" "${licenses}"
# The real file under the name the loader opens (the release ships symlinks).
install -m 0755 "${src}/lib/libonnxruntime.so.${VERSION}" "${dest}/libonnxruntime.so"
install -m 0644 "${src}/ThirdPartyNotices.txt" "${licenses}/onnxruntime-linux-ThirdPartyNotices.txt"
echo "${VERSION}" > "${marker}"
echo "ONNX Runtime ${VERSION} installed in ${dest}"
