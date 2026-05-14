# MeetAI — Build Status

**Last updated:** 2026-05-14  
**App location:** `/home/hussain/Programming/fypmeet/fydp/meetaiapp/`  
**Plan file:** `/home/hussain/.claude/plans/make-implementation-plan-for-snappy-pebble.md`  
**Design HTML:** `/home/hussain/Programming/fypmeet/fydp/design/meetai-ui.html`  
**Reference repos:** `/home/hussain/Programming/fypmeet/refernceprojext/{meetily,openoats,natively,natively-fork,screenpipe,obsidian-voice-notes}`

---

## How to run

```bash
cd /home/hussain/Programming/fypmeet/fydp/meetaiapp
bun tauri dev
```

First compile takes ~10 min (whisper.cpp C++ build). Subsequent runs are incremental (~30s).

**Prereq:** `clang` must be installed — `sudo pacman -S clang` (needed once for whisper-rs bindgen).

---

## Milestones

| # | Name | Status |
|---|------|--------|
| M0 | Design HTML | ✅ Done |
| M1 | Scaffold + DB + Settings screen | ✅ Done |
| M2 | KB ingestion + embeddings + watcher | ✅ Done |
| M3 | Audio capture + live Whisper + Live Session screen | ✅ Done |
| M4 | KB nudge loop + AI suggestions | ✅ Done |
| M5 | Library + chat with meetings | ✅ Done |
| M6 | Post-meeting pipeline + Post-Meeting screen + export | ✅ Done |
| M7 | Live Overlay window + content protection | ✅ Done |
| M8 | Onboarding + auto-detect | ✅ Done |
| M9 | Polish: cold start, memory, telemetry | ✅ Done |
| M10 | MCP server | ✅ Done |
| M11 | Parakeet sidecar | ⬜ Next |

---

## Hard constraints (must never break)

- Groq is the **only** LLM provider — `llama-3.1-8b-instant` (live/nudge) and `llama-3.3-70b-versatile` (post-meeting summarize). No Ollama, no OpenAI, no provider picker.
- All data stays in local SQLite only. No cloud storage.
- All embeddings run locally via fastembed. No embedding API calls.
- Tauri v2 only.

---

## Current AppState

```rust
pub struct AppState {
    pub pool: SqlitePool,
    pub data_dir: PathBuf,              // app_data_dir()
    pub embed_model: EmbedModel,        // Arc<Mutex<Option<TextEmbedding>>>
    pub watcher_task: Mutex<Option<JoinHandle<()>>>,
    pub whisper_model: WhisperModel,    // Arc<Mutex<Option<Arc<WhisperContext>>>>
    pub active_session: Mutex<Option<ActiveSession>>,
    pub job_tx: mpsc::Sender<JobRequest>,  // post-meeting job queue
}
```

---

## Current Cargo deps

```toml
tauri = "2", tauri-plugin-{opener,sql,dialog} = "2"
serde + serde_json = "1"
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio-rustls"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
anyhow = "1"
fastembed = "4"
notify = "7"
sha2 = "0.10", hex = "0.4", walkdir = "2"
cpal = "0.15"
hound = "3"
whisper-rs = "0.16"   # NOTE: 0.13 does NOT work (C API mismatch)
uuid = { version = "1", features = ["v4"] }
```

---

## Current routes

| Path | Component | Notes |
|------|-----------|-------|
| `/library` | `Library.tsx` | Default landing page |
| `/chat` | `Chat.tsx` | |
| `/settings` | `Settings.tsx` | |
| `/live` | `LiveSession.tsx` | Setup panel → recording panel |
| `/meeting/:id` | `PostMeeting.tsx` | 4-tab post-meeting view |

---

## Current Tauri commands

| Command | Returns | Added |
|---------|---------|-------|
| `settings_get` | `Settings` | M1 |
| `settings_set(key, value)` | `()` | M1 |
| `groq_test_connection(key)` | `bool` | M1 |
| `kb_index_start(folder)` | `()` | M2 |
| `kb_reindex_all()` | `()` | M2 |
| `kb_search(query, top_k?)` | `Vec<SearchResult>` | M2 |
| `audio_devices_list()` | `Vec<DeviceInfo>` | M3 |
| `whisper_model_status()` | `WhisperStatus` | M3 |
| `whisper_download_model(model_name)` | `()` | M3 |
| `meeting_start(title, platform?)` | `meeting_id: String` | M3 |
| `meeting_stop()` | `meeting_id: String` | M3 |
| `meetings_list()` | `Vec<MeetingRow>` | M5 |
| `meeting_search(query)` | `Vec<MeetingRow>` | M5 |
| `chat_query(question)` | `ChatResponse` | M5 |
| `meeting_get(id)` | `MeetingDetail` | M6 |
| `action_item_toggle(id, done)` | `()` | M6 |
| `meeting_notes_save(id, notes)` | `()` | M6 |
| `meeting_export_markdown(id)` | `String` | M6 |
| `meeting_regenerate_summary(id)` | `()` | M6 |
| `overlay_show()` | `()` | M7 |
| `overlay_hide()` | `()` | M7 |
| `overlay_toggle()` | `()` | M7 |
| `auto_start_enable()` | `()` | M8 |
| `auto_start_disable()` | `()` | M8 |
| `log_file_path()` | `String` | M9 |
| `mcp_snippet()` | `String` | M10 |

---

## Current Tauri events emitted (Rust → frontend)

| Event | Payload | Source |
|-------|---------|--------|
| `kb-index-progress` | `{ current, total, file, done }` | KB indexer |
| `whisper-download-progress` | `{ model, downloaded, total, percent, done }` | STT module |
| `transcript-segment` | `{ meeting_id, source, text, start_ms, end_ms, is_final }` | Audio engine |
| `nudge-update` | `{ id, file_path, breadcrumb, snippet, score, suggestion }` | Nudge engine |
| `job-progress` | `{ meeting_id, kind, status, error }` | Job queue |
| `obsidian-export-done` | `{ meeting_id }` | Summarize job |

---

## M0 — Design HTML ✅

Single-file design spec at `design/meetai-ui.html`.  
Palette: `--paper #f5f2ec`, `--ink #0d0d0f`, `--amber #e8a445`, `--green #2d7a5a`, `--blue #2a5fac`, `--red #c83c3c`.  
Fonts: Instrument Serif (headings), DM Mono (labels/code), Geist (UI).

---

## M1 — Scaffold + DB + Settings screen ✅

- Tauri v2 + React 19 + Vite + TypeScript + Tailwind v4
- Zustand + React Router (MemoryRouter)
- sqlx 0.8 (sqlite, async) — uses `sqlx::query().bind()` non-macro form (no DATABASE_URL needed)
- Full DB schema in `src-tauri/migrations/0001_init.sql`
- Settings screen: Groq key + Test button, KB folder picker, all toggles/sliders

**DB tables:** `settings`, `meetings`, `transcript_segments`, `segments_fts` (FTS5), `summaries`, `action_items`, `kb_files`, `kb_chunks` (embedding BLOB), `meeting_chunks` (embedding BLOB), `jobs`, `pre_context_chunks`, `meetings_fts` (FTS5).

---

## M2 — KB ingestion + embeddings + watcher ✅

- `src-tauri/src/kb/chunker.rs` — markdown-aware 80–500 word chunks, 20% overlap, header breadcrumbs (port of OpenOats KnowledgeBase.swift:547)
- `src-tauri/src/kb/embed.rs` — fastembed bge-small-en-v1.5 (384-dim, CPU); lazy init; `spawn_blocking`
- `src-tauri/src/kb/index.rs` — SHA256 skip-unchanged, batch embed, emits `kb-index-progress`
- `src-tauri/src/kb/search.rs` — brute-force cosine similarity (no sqlite-vec needed up to ~50k chunks)
- `src-tauri/src/kb/watcher.rs` — notify crate, std::thread, tokio mpsc bridge, 500ms debounce

---

## M3 — Audio capture + live Whisper + Live Session screen ✅

- `src-tauri/src/audio/mod.rs` — AudioSource, `resample_to_16k` (linear interp), `to_mono`
- `src-tauri/src/audio/mic.rs` — cpal mic capture, F32/I16 format, `capture_loop` on std::thread
- `src-tauri/src/audio/system.rs` — Linux PulseAudio monitor via cpal; skips gracefully if none found
- `src-tauri/src/audio/vad.rs` — energy VAD: threshold_on=0.015, threshold_off=0.008, redemption=1.5s, min=250ms, 200ms pre-roll
- `src-tauri/src/stt/whisper.rs` — **whisper-rs 0.16** (0.13 has C API mismatch); lazy load; transcribe in `spawn_blocking`; HuggingFace model download with progress events
- `src-tauri/src/meeting/session.rs` — session lifecycle; WAV writer thread (hound, 16kHz f32); engine tokio task; per-source VAD → Whisper; emits `transcript-segment` + DB writes; `recover_interrupted()` on startup

**First use:** download Whisper model in-app (ggml-tiny.en.bin, ~75 MB from HuggingFace) before starting first meeting.

---

## M4 — KB nudge loop + AI suggestions ✅

- `src-tauri/src/llm/mod.rs` — `chat()` non-streaming Groq call (json: true, 15s timeout)
- `src-tauri/src/nudge/mod.rs` — `NudgeSettings` struct; engine loop (100ms ticks, stops with AtomicBool); queries last 40 words from DB → embeds → KB search → score threshold → Jaccard dedup (>0.7 vs last 3 cards) → optional Groq talking point (12s timeout) → emits `nudge-update`
- `meeting/session.rs` updated — starts nudge engine alongside audio engine; stop waits for both
- `lib.rs` updated — passes `NudgeSettings` to `start_session`
- `src/stores/session.ts` — `nudgeCards: NudgeCard[]` (max 3, newest first)
- `src/routes/LiveSession.tsx` — NudgeCardView (opacity 100%/60%/30%), listens to nudge-update

---

## M5 — Library + chat with meetings ✅

- `src-tauri/src/meeting/library.rs` — `list_meetings`, `search_meetings` (LIKE on title+transcript), `build_chat_context` (finds relevant segments, formats as `[Meeting: …]\n[ts] Speaker: text`)
- New commands: `meetings_list`, `meeting_search`, `chat_query` (30s timeout, 8b-instant, returns `{ answer, sources }`)
- `src/routes/Library.tsx` — grouped by day, LIKE search with 300ms debounce, status badges, navigate to `/meeting/:id` on click
- `src/routes/Chat.tsx` — conversation UI, user/assistant bubbles, source citation tags, starter prompts
- App opens on `/library` by default

---

## M6 — Post-meeting pipeline + Post-Meeting screen + export ✅

### Job queue

`src-tauri/src/meeting/jobs.rs` — sequential tokio mpsc queue (buffer 64). Started in app `setup()`. Sender stored in AppState as `job_tx`. `meeting_stop` enqueues `Embed` then `Summarize`.

**Embed job:** chunks transcript into ~200-word windows (speaker-labelled), embeds via fastembed, writes `meeting_chunks`. Initialises embed model if not already loaded.

**Summarize job:** builds full transcript → calls `llama-3.3-70b-versatile` with structured JSON prompt → strips markdown fences → parses `{ overview, decisions[], action_items[], topics[] }` → writes `summaries` + `action_items`. If `obsidian_vault` set in settings, auto-writes `{vault}/Meetings/{title}.md`. Marks meeting `status='done'`.

`session.rs` also now records `duration_ms = ended_at - started_at` on stop.

### New commands (M6)

- `meeting_get(id)` → `MeetingDetail` (meeting + summary + action_items + segments + jobs)
- `action_item_toggle(id, done)` → updates `action_items.done`
- `meeting_notes_save(id, notes)` → updates `meetings.notes`
- `meeting_export_markdown(id)` → Markdown string with Obsidian frontmatter
- `meeting_regenerate_summary(id)` → enqueues new Summarize job

### Post-Meeting screen

`src/routes/PostMeeting.tsx` — 4 tabs:
- **Summary** — job progress badges (Embed/Summarize), overview, decisions list, action items with checkboxes (optimistic toggle), topic chips, Regenerate button; auto-refreshes on `job-progress` events
- **Transcript** — speaker-coloured rows (blue=You, green=Speaker) with `mm:ss` timestamps
- **Notes** — textarea, auto-saves to DB 600ms after typing stops
- **Export** — "Download .md" (client-side blob) + "Copy to Clipboard"

---

## M7 — Live Overlay window + content protection ✅

- `tauri-plugin-global-shortcut = "2"` added to Cargo.toml
- Overlay window created programmatically in `lib.rs` setup:
  - Dev URL: `http://localhost:1420/overlay.html`; prod: `WebviewUrl::App("overlay.html")`
  - 380×220, no decorations, transparent, always-on-top, skip taskbar, hidden at start
- Global shortcut `Ctrl+Shift+O` registered (best-effort; silently skips on Wayland)
- Plugin handler in `Builder::with_handler` toggles overlay show/hide
- `meeting_start` calls `set_content_protected(true)` when `screen_share_protection` setting is on (no-op on Linux)
- `meeting_stop` calls `set_content_protected(false)` and auto-hides overlay
- Commands: `overlay_show`, `overlay_hide`, `overlay_toggle`
- `capabilities/overlay.json` — `core:default` for overlay window
- Vite MPA: `vite.config.ts` adds `rollupOptions.input` for both `index.html` + `overlay.html`
- `overlay.html` at project root; `src/overlay/main.tsx` + `src/overlay/Overlay.tsx`
- Overlay UI: last 3 transcript lines (fading opacity), current nudge card, Expand + Stop buttons
- LiveSession recording panel has "Overlay" button in header (calls `overlay_toggle`)
- Linux note: `set_content_protected` is a no-op (Wayland/X11 limitation)

---

## M8 — Onboarding + auto-detect ✅

- `App.tsx`: on mount, calls `settings_get()`. If `groq_key` is empty → `initialRoute = "/onboarding"`; otherwise `/library`. Brief "Loading…" screen shown during the check.
- `src/routes/Onboarding.tsx` — 3-step wizard:
  1. Welcome — description + feature list
  2. Groq key — password input, Test button, link opens console.groq.com/keys via plugin-opener, "Next (skip test)" allowed
  3. KB folder — optional folder picker, "Finish" starts indexing and navigates to `/library`
- `src-tauri/src/audio/autodetect.rs` — new module: opens default input device via cpal, collects audio RMS every 2 s, emits `meeting-detected` event after 10 s of sustained audio > threshold; resets when audio drops
- `AppState.auto_detect: Mutex<Option<AutoDetectHandle>>` — started in `setup()` if `auto_start` was previously enabled
- Commands `auto_start_enable`, `auto_start_disable` — start/stop the detection thread
- Settings.tsx auto_start toggle now also calls `ipc.autoStartEnable/Disable()`
- `App.tsx` listens to `meeting-detected` event → shows `DetectedBanner` (fixed top banner with title input, Start button → navigates to `/live`, dismiss button)
- Linux note: all audio capture via cpal, no OS-level notification; banner appears in-app

---

## M9 — Polish: cold start, memory, telemetry ✅

- Added `log = "0.4"` + `simplelog = "0.12"` to Cargo.toml
- `static LOG_PATH: OnceLock<PathBuf>` in lib.rs — set in setup, read by panic hook
- `install_panic_hook()` called before Tauri Builder in `run()` — writes panics to log file with unix timestamp
- `simplelog::WriteLogger` init in setup: WARN+ to `app_data_dir/logs/meetai.log` (append mode, creates file)
- `log_file_path()` command returns the log path for the frontend to open
- Settings › "Version & logs" section added:
  - Build info table (version, LLM, embeddings, transcription, platform)
  - Log file path display + "Open log file" button (uses `openPath` from plugin-opener)
  - Cold start breakdown: what runs at launch vs. on first use
  - Memory characteristics documented (ring-buffer drop policy, disk-streamed WAV, max 3 nudge cards in RAM)
- **Cold start audit** (all already lazy before this milestone):
  - Whisper model: loaded only when `meeting_start` called
  - fastembed: loaded only on first KB op
  - DB init: fast (migrations + a few settings rows)
  - Auto-detect thread: spawns only if `auto_start` setting was previously enabled

---

## M10 — MCP server ✅

- `src-tauri/src/bin/meetai_mcp.rs` — standalone stdio MCP server binary (no Tauri dependency)
  - Reads newline-delimited JSON-RPC 2.0 from stdin, writes responses to stdout
  - MCP protocol v2024-11-05: handles `initialize`, `ping`, `tools/list`, `tools/call`, `resources/list`, `prompts/list`
  - DB path: `--db /path/to/meetai.db` arg, falls back to `$XDG_DATA_HOME/com.hussain.meetai/meetai.db`
  - Opens SQLite read-only via sqlx (WAL mode supports concurrent read from running app)
  - 4 tools: `search_meetings`, `get_meeting_transcript`, `get_action_items`, `get_recent_meetings`
  - No external deps beyond existing Cargo.toml (sqlx, serde_json, tokio, anyhow)
  - Build: `cargo build --release --bin meetai_mcp` → binary at `target/release/meetai_mcp`
- `mcp_snippet()` command — returns ready-to-paste JSON config for Claude Desktop
  - Uses `current_exe().parent().join("meetai-mcp")` for binary path + `data_dir/meetai.db` for DB path
- Settings › Integrations MCP toggle now shows `McpSnippetBlock` when enabled:
  - Code block with the pre-filled JSON config
  - "Copy config" button
  - One-liner build command shown inline

**To use with Claude Desktop:**
1. `cargo build --release --bin meetai_mcp`
2. Enable MCP in Settings → Integrations
3. Copy the config snippet shown → paste into `~/.config/Claude/claude_desktop_config.json`
4. Restart Claude Desktop → four MeetAI tools appear

---

## M11 — What to build next (final milestone)

**Goal:** Higher-accuracy post-meeting retranscription via Parakeet TDT sidecar.

This milestone is optional and complex (Python + PyInstaller packaging). Deferred to after the app is running end-to-end.

---

## Environment

- OS: Arch Linux
- Rust: 1.95.0 via rustup (`~/.cargo/bin/`)
- Bun: 1.3.11
- clang: installed (required by whisper-rs bindgen)
- Tauri Linux deps: webkit2gtk-4.1, gtk3, libappindicator-gtk3, librsvg, libsoup3, base-devel, openssl, pkgconf, libayatana-appindicator — all installed
- xdotool: NOT installed (only needed for X11 global shortcuts in M7)
- cmake: 4.3.x installed (required by whisper-rs)
