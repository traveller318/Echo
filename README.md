# Echo

**Local, private speech-to-text for Windows.** Hold a hotkey, speak, release — clean, punctuated text is typed into whatever app you are using.

Speech recognition and text cleanup both run on your own machine. There is no account, no subscription, no usage limit and no telemetry. Your audio and your words never leave your computer.

---

## How it works

### Using it

1. **Hold `Ctrl+Alt`** in any app (an email, a chat, a code editor) and start talking.
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
| **4. Clean up** | Rules remove fillers ("um", "uh"), repeated words and spacing mistakes, write spoken numbers as digits ("twenty five" → "25"), and apply your dictionary spellings. If grammar polish is on, a local language model then smooths the wording. |
| **5. Deliver** | The text is pasted into the window you were using and kept on your clipboard. If that window can't accept a paste (for example an app running as administrator), Echo copies the text and tells you to press `Ctrl+V`. |
| **6. Save** | The take is saved to History, where you can search it, copy it or re-run it. Your Dashboard stats are updated. |

All six steps run on your computer. Nothing is sent over the internet.

## Features

- **Fast local recognition** — NVIDIA Parakeet TDT 0.6B v3 (int8 ONNX) on ONNX Runtime. Runs on the CPU, with optional GPU acceleration through DirectML. Supports 25 languages with automatic punctuation and capitalization.
- **Text cleanup** — a rule pass that is always on removes filler words and repeats, writes spoken numbers as digits, and fixes spacing and casing. An optional grammar polish uses a local Qwen3 1.7B model through llama.cpp.
- **Personal dictionary** — save names, jargon and spellings so they come out right every time.
- **Works in any app** — text is inserted into the focused window. Paste the last transcript again at any time with `Ctrl+Alt+V`.
- **Never lose a take** — audio is written to disk while you speak. If anything fails or crashes, the recording is still in History and can be re-run.
- **History** — a searchable list of past dictations, with configurable retention for text and recordings.
- **Dashboard** — time saved, words dictated, speaking speed (WPM), activity over time and latency.
- **Model manager** — download, import from a folder or switch speech and polish models.
- **Customizable** — hold-to-talk or toggle mode, rebindable hotkeys, microphone choice, sound cues, pill position, light/dark/system theme, launch at startup, start in the tray.
- **Offline mode** — one switch to guarantee Echo never touches the network.

## Hotkeys

| Action | Default |
|---|---|
| Dictate (hold to talk, or toggle) | `Ctrl+Alt` |
| Paste last transcript | `Ctrl+Alt+V` |
| Cancel the current take | `Esc` |

Dictation and paste-last can be rebound in **Settings**.

## Privacy

- All speech recognition and text processing happen locally. No cloud AI, no remote inference.
- The network is used for exactly two things, and only when you start them: downloading a model (Hugging Face) and downloading the optional grammar runtime (a pinned llama.cpp release). **Offline mode** turns both off, and models can always be imported from disk instead.
- No analytics, no crash uploads, no update checks. Logs stay on your machine and never contain what you said.

## Getting started

There are no prebuilt releases — Echo is built from source.

### Requirements

- Windows 10 (1809 or later) or Windows 11, x64
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with **Desktop development with C++** and the **Windows SDK**
- [Rust](https://rustup.rs) — the exact toolchain is pinned in `src-tauri/rust-toolchain.toml` and installed automatically by rustup
- [Node.js](https://nodejs.org) 22.13 or later
- [pnpm](https://pnpm.io) 11

### Run and build

```powershell
pnpm install
pnpm tauri dev      # run the app in development mode
pnpm tauri build    # build an installer into src-tauri/target/release/bundle/nsis/
```

The installer is per-user, so it needs no admin rights. It is not code-signed, so Windows SmartScreen may warn you — choose **More info → Run anyway**.

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
| Speech | ONNX Runtime (CPU / DirectML), Parakeet TDT 0.6B v3, Silero VAD v5 |
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
