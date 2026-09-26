<!--
  SOURCE OF TRUTH KEYWORDS: README, install, privacy, build from source, local gate, licenses, third-party credits
  WHAT:  The user-facing page of the repository: how to install Echo, what stays private, how to build and test it.
  WHY:   The repository must stand on its own: it links to nothing outside the tracked files.
  WHERE: GitHub repository front page.
-->

# Echo

Local, private speech-to-text for Windows. Press a hotkey, speak, press it again, and polished text is pasted into the app you are using. Speech recognition and text cleanup run entirely on your machine: no account, no subscription, no usage limit, no telemetry.

## Install

1. Run `Echo_<version>_x64-setup.exe`. It installs for your Windows user only, so no administrator prompt appears.
2. The installer is not code-signed yet. If Windows SmartScreen warns you, choose **More info → Run anyway**.
3. If WebView2 is missing (rare, older Windows 10), the installer adds it.
4. To upgrade, run a newer installer over the old one. History and settings are kept.

Requires Windows 10 1809 or later, or Windows 11, on x64.

## Privacy

- Audio and text never leave your computer. Recognition (NVIDIA Parakeet) and cleanup run locally.
- Echo only goes online when you ask it to: downloading a speech model (Hugging Face) or the optional grammar model and its runtime (the pinned llama.cpp release).
- **Offline mode** turns both off. Models can always be imported from a folder instead.
- There are no analytics, no crash uploads and no update checks. Logs stay on your machine and never contain what you said.

## Build from source

Prerequisites:

| Tool | Version |
|---|---|
| Visual Studio Build Tools | "Desktop development with C++" workload + Windows 10/11 SDK |
| Rust | Installed through [rustup](https://rustup.rs); `src-tauri/rust-toolchain.toml` selects the exact toolchain |
| Node.js | 22.13 or later |
| pnpm | 11 (see `packageManager` in `package.json`) |

```powershell
pnpm install
pnpm tauri dev          # run Echo in development mode
```

The speech-recognition tests run the real Parakeet model, which is too large for the repository. Install it once before running the tests: start Echo with `pnpm tauri dev` and download the model during setup (or on the Models page). It lands in `%LOCALAPPDATA%\app.echo.desktop\models`, where the tests read it.

Before any build, the local gate must pass:

```powershell
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cd ..
pnpm tsc --noEmit
pnpm lint
pnpm test
```

Then build the installer:

```powershell
pnpm tauri build        # installer lands in src-tauri/target/release/bundle/nsis/
```

## License

Echo is MIT licensed. See [`LICENSE`](LICENSE).

Echo ships third-party components under their own licenses: ONNX Runtime (MIT), DirectML (Microsoft Software License Terms), the Microsoft Visual C++ runtime (Visual Studio distributable code), the Silero VAD model (MIT), and the Inter and Poppins typefaces (SIL Open Font License 1.1). Their license texts are in [`src-tauri/resources/licenses`](src-tauri/resources/licenses) and [`src/styles/fonts`](src/styles/fonts), and the installer places them in its `licenses` folder. Models you download (Parakeet, CC BY 4.0; Qwen3, Apache 2.0) and the llama.cpp runtime (MIT) are credited on their cards on the app's Models page.
