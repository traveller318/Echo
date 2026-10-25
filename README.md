# Echo

**Local, private speech-to-text for Windows and Linux.** Hold a hotkey, speak, release — clean, punctuated text is typed into whatever app you are using.

Speech recognition and text cleanup both run on your own machine. There is no account, no subscription, no usage limit and no telemetry. Your audio and your words never leave your computer.

---

## How it works

### Using it

1. **Hold `Ctrl+Alt`** (`Ctrl+Alt+Space` on Linux) in any app (an email, a chat, a code editor) and start talking.
2. **Watch the pill.** A small glass pill appears near the bottom of the screen with a live waveform, so you know Echo is listening. It never takes focus away from the app you are typing in.
3. **Release the keys.** A moment later the finished text appears where your cursor was.

Prefer not to hold keys? Switch to **toggle mode** in Settings: press once to start and again to stop.
Changed your mind? Press `Esc` to start a 3-second countdown that throws the recording away, or press `Esc` again to keep going.

### What happens behind the scenes

| Step | What Echo does |
|---|---|
| **1. Record** | Captures your microphone and saves the audio to disk as you speak, so a recording is never lost even if something crashes. |
| **2. Detect speech** | A voice-activity model (Silero VAD) splits the audio into speech segments and skips the silence. |
| **3. Transcribe** | The speech model (Parakeet) turns each segment into punctuated, capitalized text. It starts while you are still talking, so the text is ready almost as soon as you stop. |
| **4. Clean up** | Rules remove fillers ("um", "uh"), repeated words and spacing mistakes, apply your dictionary spellings, and write what you said the way you'd type it: numbers, amounts, emails, web addresses, symbols and numbered lists (see [Smart formatting](#smart-formatting)). If grammar polish is on, a local language model then smooths the wording without undoing any of that. |
| **5. Deliver** | The text is pasted into the window you were using and kept on your clipboard. If that window can't accept a paste (for example an app running as administrator), Echo copies the text and tells you to press `Ctrl+V`. |
| **6. Save** | The take is saved to History, where you can search it, copy it or re-run it. Your Dashboard stats are updated. |

All six steps run on your computer. Nothing is sent over the internet.

## Features

- **Fast local recognition** — NVIDIA Parakeet TDT 0.6B v3 (int8 ONNX) on ONNX Runtime. Runs on the CPU, with optional GPU acceleration through DirectML. Supports 25 languages with automatic punctuation and capitalization.
- **Text cleanup** — a rule pass that is always on removes filler words and repeats and fixes spacing and casing. An optional grammar polish uses a local Qwen3 1.7B model through llama.cpp.
- **Smart formatting** — spoken numbers, amounts, emails, web addresses, symbols and lists come out written, not spelled out (below).
- **Personal dictionary** — save names, jargon and spellings so they come out right every time.
- **Works in any app** — text is inserted into the focused window. Paste the last transcript again at any time with `Ctrl+Alt+V`.
- **Never lose a take** — audio is written to disk while you speak. If anything fails or crashes, the recording is still in History and can be re-run.
- **History** — a searchable list of past dictations, with configurable retention for text and recordings.
- **Dashboard** — time saved, words dictated, speaking speed (WPM), activity over time and latency.
- **Model manager** — download, import from a folder or switch speech and polish models.
- **Customizable** — hold-to-talk or toggle mode, rebindable hotkeys, microphone choice, sound cues, pill position, light/dark/system theme, launch at startup, start in the tray.
- **Offline mode** — one switch to guarantee Echo never touches the network.

## Smart formatting

| You say | Echo types |
|---|---|
| "twenty five", "ten thirty", "three point five", "fifty percent" | `25`, `10:30`, `3.5`, `50%` |
| "twenty five dollars and fifty cents", "two hundred rupees" | `$25.50`, `₹200` |
| "john at gmail dot com", "google dot com slash docs" | `john@gmail.com`, `google.com/docs` |
| "snake underscore case", "hashtag launch", "open bracket x close bracket" | `snake_case`, `#launch`, `(x)` |
| "The list is as follows. First apple, second oranges, third mangoes." | `The list is as follows`<br>`1. apple`<br>`2. oranges`<br>`3. mangoes` |

Rules only fire on a clear match, so everyday phrases ("five apples", "wait a second", "look at that") stay as spoken. **Spoken symbols** and **Format spoken lists** can be turned off in **Settings → Cleanup**.

## Hotkeys

| Action | Default |
|---|---|
| Dictate (hold to talk, or toggle) | `Ctrl+Alt` (`Ctrl+Alt+Space` on Linux) |
| Paste last transcript | `Ctrl+Alt+V` |
| Cancel the current take | `Esc` |

Dictation and paste-last can be rebound in **Settings**.

## Privacy

- All speech recognition and text processing happen locally. No cloud AI, no remote inference.
- The network is used for exactly two things, and only when you start them: downloading a model (Hugging Face) and downloading the optional grammar runtime (a pinned llama.cpp release). **Offline mode** turns both off, and models can always be imported from disk instead.
- No analytics, no crash uploads, no update checks. Logs stay on your machine and never contain what you said.

## Getting started

- **Linux** — install a prebuilt package with one command, or build from source: see [Linux](#linux).
- **Windows** — Echo is built from source: see [Windows](#windows).

## Linux

Echo runs on x86_64 Linux under **X11 and Wayland**. Everything stays on your machine, exactly as on Windows.

### Install

```bash
curl -fsSL https://raw.githubusercontent.com/traveller318/Echo/main/scripts/install.sh | bash
```

The script picks the right package for your distribution (`.deb` for Debian, Ubuntu and relatives; `.rpm` for Fedora, RHEL and openSUSE; the AppImage in your home folder for everything else), downloads it with the release's `SHA256SUMS`, **checks the checksum before installing anything**, and installs it. The only privileged step is your package manager's own install. Prefer to do it by hand? Download the package from the Releases page and install it with `apt install ./Echo_*.deb`, `dnf install ./Echo-*.rpm`, or `chmod +x Echo_*.AppImage && ./Echo_*.AppImage`. Remove it with `scripts/uninstall.sh` (add `--purge` to delete your history and models too).

### How it works on each display server

Linux is stricter than Windows about which program may read keys and type into other windows. Echo asks each desktop through its official mechanism instead of reading `/dev/input`, so it needs **no root access, no udev rule and no `input` group membership**.

| | X11 | Wayland — GNOME / KDE | Wayland — Hyprland / Sway / river |
|---|---|---|---|
| **Dictation hotkey** | Works anywhere, including hold-to-talk. | The desktop's shortcut portal asks you once to approve the key. | Bind a key yourself (below). |
| **Typing the text** | Synthetic `Ctrl+V` (`Ctrl+Shift+V` in terminals). | The desktop asks you once to allow keyboard control. | The compositor's virtual-keyboard protocol, no prompt. |
| **If typing is unavailable** | The text is copied and a toast asks you to press `Ctrl+V`. | same | same |

The default dictation hotkey on Linux is **`Ctrl+Alt+Space`**. A bare `Ctrl+Alt` (the Windows default) is only possible on X11; you can choose it in Settings there. Hold-to-talk needs the desktop to report key release: KDE Plasma and Hyprland do, GNOME's shortcut portal does not, so on GNOME Echo uses toggle mode (press to start, press to stop).

**Desktops without a shortcut portal** (Sway, river, others), or if you prefer your compositor to own the key: bind any key to one of these commands. Echo must already be running (turn on *Launch at startup* in Settings).

| Command | Does |
|---|---|
| `echo-dictation --toggle` | Starts dictating, or stops the take that is recording. |
| `echo-dictation --stop` | Stops the recording. |
| `echo-dictation --cancel` | Same as `Esc`: starts the cancel countdown. |

**Hyprland** names a portal shortcut after the app and its id, so bind it with the `global` dispatcher, for example in the Lua config:

```lua
hl.bind("CTRL + ALT + SPACE", hl.dsp.global("app.echo.desktop:record"))
hl.bind("CTRL + ALT + V", hl.dsp.global("app.echo.desktop:paste-last"))
```

(`hyprctl globalshortcuts` lists the names Echo registered.) In the classic config format this is `bind = CTRL ALT, SPACE, global, app.echo.desktop:record`.

### Known limits on Linux

- A key bound on X11 still reaches the focused app (Echo listens without grabbing keys, so nothing is swallowed).
- On Wayland the floating "pill" is placed by your compositor, not by Echo, because Wayland does not let a program position its own window. On Hyprland you can pin it with a window rule.
- The tray icon needs an AppIndicator host; GNOME ships none by default (install the *AppIndicator and KStatusNotifierItem Support* extension).
- Terminals on Wayland get `Ctrl+V`, which most do not treat as paste; use `Ctrl+Shift+V` yourself or paste from the clipboard (the text is always on it).
- Grammar polish downloads a CPU build of llama.cpp on Linux (no GPU offload).

### Build from source on Linux

```bash
# Debian / Ubuntu
sudo apt install build-essential curl git libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev libssl-dev patchelf
# Fedora
sudo dnf install gcc-c++ git webkit2gtk4.1-devel gtk3-devel libayatana-appindicator-gtk3-devel librsvg2-devel alsa-lib-devel openssl-devel patchelf

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust; the exact toolchain is pinned in src-tauri/rust-toolchain.toml
# Node.js 22.13+ and pnpm 11 (npm install -g pnpm@11)

git clone https://github.com/traveller318/Echo.git && cd Echo
./scripts/fetch-onnxruntime-linux.sh      # the pinned ONNX Runtime, checksum-verified
pnpm install
pnpm tauri dev                            # run it
pnpm tauri build                          # .deb, .rpm and .AppImage in src-tauri/target/release/bundle/
```

## Windows

There are no prebuilt Windows releases yet — build from source.

### Requirements

- Windows 10 (1809 or later) or Windows 11, x64
- [Git](https://git-scm.com)
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with **Desktop development with C++** and the **Windows SDK**
- [Rust](https://rustup.rs) — the exact toolchain is pinned in `src-tauri/rust-toolchain.toml` and installed automatically by rustup
- [Node.js](https://nodejs.org) 22.13 or later
- [pnpm](https://pnpm.io) 11

### 1. Install the tools (one time)

In PowerShell:

```powershell
winget install --id Git.Git -e
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install --id Rustlang.Rustup -e
winget install --id OpenJS.NodeJS.LTS -e
```

Close and reopen PowerShell so the new tools are on your `PATH`, then:

```powershell
npm install -g pnpm@11
```

### 2. Get the code and install dependencies

```powershell
git clone https://github.com/traveller318/Echo.git
cd Echo
pnpm install
```

The first Rust build downloads the pinned toolchain and compiles every crate, so it takes several minutes.

### 3. Run or install

```powershell
pnpm tauri dev      # run the app in development mode
pnpm tauri build    # build an installer into src-tauri/target/release/bundle/nsis/
```

To install it as a normal desktop app, run the built installer:

```powershell
& (Get-ChildItem src-tauri\target\release\bundle\nsis\*-setup.exe | Select-Object -First 1).FullName
```

The installer is per-user, so it needs no admin rights, and it adds Start menu and desktop shortcuts. It is not code-signed, so Windows SmartScreen may warn you — choose **More info → Run anyway**.

On first launch, a short setup picks your microphone, downloads the speech model (or imports it from a folder) and lets you try a first dictation.

### Checks

```powershell
pnpm lint                                   # ESLint
pnpm test                                   # frontend tests (Vitest)
pnpm build                                  # TypeScript type check + frontend build
cd src-tauri; cargo clippy; cargo test      # Rust lints and tests (includes architecture rules)
```

## Tech stack

| Layer | Technology |
|---|---|
| App shell | [Tauri 2](https://tauri.app) |
| Backend | Rust, SQLite (local history database) |
| Speech | ONNX Runtime (CPU; DirectML on Windows), Parakeet TDT 0.6B v3, Silero VAD v5 |
| Grammar polish | llama.cpp server, Qwen3 1.7B (Q4_K_M) — optional |
| Frontend | React 19, TypeScript, Tailwind CSS, shadcn/ui, Zustand, TanStack Query, Recharts |
| Type bridge | specta — TypeScript bindings are generated from the Rust types |

## Architecture

The Rust backend is organized in layers, and dependencies only point downward. An automated test (`src-tauri/tests/architecture.rs`) fails the build if a layer imports one above it.

| Layer | Folder | Role |
|---|---|---|
| Registry | `src-tauri/src/registry/` | Single source of truth for what the app has: engines, models, settings, hotkeys, navigation, metrics, permissions. |
| IPC | `src-tauri/src/ipc/` | A command factory that every frontend command goes through; it handles validation, permissions, tracing, errors and metrics in one place. |
| Pipeline | `src-tauri/src/pipeline/` | Business logic: the recording session state machine, capture, transcription, cleanup, delivery, recovery and retention. |
| Ports | `src-tauri/src/ports/` | Traits for everything swappable: speech model, audio, clipboard, text insertion, hotkeys, GPU and more. |
| Adapters | `src-tauri/src/adapters/` | Concrete implementations behind the ports, including the ONNX speech engine, llama.cpp polish and all Windows APIs. |
| Services | `src-tauri/src/services/` | The only code that touches the database. |
| Types | `src-tauri/src/types/` | Shared domain types. |

Because the core only talks to ports, adding a new speech or language model means writing one adapter and one registry entry — nothing else changes.

The frontend lives in `src/`: routes for Dashboard, History, Dictionary, Models, Settings and onboarding, plus the separate pill window in `src/pill/`. Settings screens are generated from the registry, and all app state is owned by Rust and pushed to the UI as typed events.
