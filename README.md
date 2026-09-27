# Echo

Local, private speech-to-text for Windows. Hold a hotkey, speak, release — clean text is typed into whatever app you are using.

Everything runs on your machine: no account, no subscription, no usage limit, no telemetry.

## Features

- **Fast local recognition** — NVIDIA Parakeet TDT 0.6B v3 on ONNX Runtime, CPU or GPU (DirectML).
- **Text cleanup** — removes fillers and repeats, fixes casing and spacing; optional grammar polish with a local Qwen3 1.7B model.
- **Works everywhere** — inserts text into the focused window; paste the last transcript again anytime.
- **History and stats** — searchable history of your dictations and a usage dashboard, stored locally.
- **Offline mode** — Echo never goes online unless you download a model; models can also be imported from a folder.

## Install

1. Download `Echo_<version>_x64-setup.exe` from Releases and run it. It installs per-user, so no admin prompt.
2. The installer is not code-signed yet. If SmartScreen warns you, choose **More info → Run anyway**.
3. On first run, setup downloads the speech model.

Requires Windows 10 1809+ or Windows 11, x64.

## Usage

| Action | Default hotkey |
|---|---|
| Dictate | Hold `Ctrl+Alt` |
| Paste last transcript | `Ctrl+Alt+V` |
| Cancel | `Esc` |

All hotkeys can be changed in Settings.

## Privacy

- Audio and text never leave your computer.
- Network is used only when you download a model (Hugging Face) or the grammar runtime (a pinned llama.cpp release).
- No analytics, no crash uploads, no update checks. Logs stay local and never contain what you said.

## Build from source

Prerequisites: Visual Studio Build Tools ("Desktop development with C++" + Windows SDK), [Rust](https://rustup.rs) (the toolchain is pinned in `src-tauri/rust-toolchain.toml`), Node.js 22.13+, pnpm 11.

```powershell
pnpm install
pnpm tauri dev      # run in development
pnpm tauri build    # installer in src-tauri/target/release/bundle/nsis/
```

Built with Tauri 2, Rust, React 19, TypeScript and Tailwind CSS. See [CONTRIBUTING.md](CONTRIBUTING.md) for the architecture and the checks every change must pass.


