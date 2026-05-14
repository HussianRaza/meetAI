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

First compile takes 2–4 minutes. Subsequent runs are fast (incremental).

---

## Milestones

| # | Name | Status |
|---|------|--------|
| M0 | Design HTML | ✅ Done |
| M1 | Scaffold + DB + Settings screen | ✅ Done |
| M2 | KB ingestion + embeddings + watcher | ✅ Done |
| M3 | Audio capture + live Whisper + Live Session screen | ✅ Done |
| M4 | KB nudge loop + AI suggestions | ⬜ Next |
| M5 | Library + chat with meetings | ⬜ Pending |
| M6 | Post-meeting pipeline + Post-Meeting screen + export | ⬜ Pending |
| M7 | Live Overlay window + content protection | ⬜ Pending |
| M8 | Onboarding + auto-detect | ⬜ Pending |
| M9 | Polish: cold start, memory, telemetry | ⬜ Pending |
| M10 | MCP server | ⬜ Pending |
| M11 | Parakeet sidecar | ⬜ Pending |

---

## M0 — Design HTML ✅

Single-file design spec at `design/meetai-ui.html` showing all 5 screens stacked:
Settings, Library+Chat, Live Session, Live Overlay, Post-Meeting, plus Onboarding.

Palette: `--paper #f5f2ec`, `--ink #0d0d0f`, `--amber #e8a445`, `--green #2d7a5a`, `--blue #2a5fac`, `--red #c83c3c`.  
Fonts: Instrument Serif (headings), DM Mono (labels/code), Geist (UI) — loaded from Bunny Fonts.

---

## M1 — Scaffold + DB + Settings screen ✅

### Stack
- Tauri v2 + React 19 + Vite + TypeScript + Tailwind v4
- State: Zustand + React Router (memory router)
- DB: sqlx 0.8 (sqlite, async) via tauri-plugin-sql
- No sqlite-vec yet (added best-effort; fallback path in M2)

### Key files

**Rust:**
- `src-tauri/src/lib.rs` — AppState, command registration
- `src-tauri/src/db/mod.rs` — SQLite pool init, runs migrations
- `src-tauri/src/settings/mod.rs` — settings_get / settings_set / test_groq
- `src-tauri/migrations/0001_init.sql` — full schema (see below)
- `src-tauri/Cargo.toml` — all dependencies
- `src-tauri/tauri.conf.json` — window: 1100×720, label "main"
- `src-tauri/capabilities/default.json` — core, opener, sql, dialog permissions

**Frontend:**
- `src/App.tsx` — MemoryRouter, routes (Settings at `/settings`)
- `src/App.css` — Tailwind v4 + design tokens + shared primitives (.input, .btn, .select, .badge)
- `src/ipc/index.ts` — typed invoke() wrappers
- `src/stores/settings.ts` — Zustand settings store
- `src/components/Sidebar.tsx` — brand, nav links, status footer
- `src/routes/Settings.tsx` — full Settings screen (Connection, KB, Recording, Nudges & AI, Privacy, Integrations)

### DB schema (0001_init.sql)
Tables: `settings`, `meetings`, `transcript_segments`, `segments_fts` (FTS5), `summaries`, `action_items`, `kb_files`, `kb_chunks` (embedding BLOB), `meeting_chunks` (embedding BLOB), `jobs`, `pre_context_chunks`, `meetings_fts` (FTS5).

vec0 virtual tables (`vec_kb_chunks`, `vec_meeting_chunks`) attempted at runtime; silently skipped if sqlite-vec not loaded.

### Tauri commands (M1)
- `settings_get` → `Settings` struct
- `settings_set(key, value)` → persists to DB
- `groq_test_connection(key)` → GET api.groq.com/openai/v1/models → bool

### Hard constraints (must not break)
- Groq is the **only** LLM provider — `llama-3.1-8b-instant` (live) and `llama-3.3-70b-versatile` (post-meeting). No Ollama, no OpenAI, no provider picker.
- All data in local SQLite only. No cloud storage.
- Tauri v2 only.

---

## M2 — KB ingestion + embeddings + watcher ✅

### Key files

**Rust:**
- `src-tauri/src/kb/mod.rs` — module root
- `src-tauri/src/kb/chunker.rs` — port of OpenOats/KnowledgeBase.swift:547; markdown-aware 80–500 word chunks with 20% overlap and header breadcrumbs
- `src-tauri/src/kb/embed.rs` — fastembed bge-small-en-v1.5 (384-dim, CPU); lazy init via `Arc<Mutex<Option<TextEmbedding>>>`; runs in `spawn_blocking`; model cached to `app_data_dir/models/`
- `src-tauri/src/kb/index.rs` — walks folder with walkdir, SHA256 skips unchanged files, batch-embeds, writes to `kb_files` + `kb_chunks`; emits `kb-index-progress` events
- `src-tauri/src/kb/search.rs` — brute-force cosine similarity (no sqlite-vec bundling needed; fine to ~50k chunks); returns top-k with file_path, breadcrumb, snippet, score
- `src-tauri/src/kb/watcher.rs` — `notify` RecommendedWatcher on a dedicated OS thread, bridges to tokio via mpsc channel, 500ms debounce, re-indexes only changed .md/.txt files

**Frontend:**
- `src/ipc/index.ts` — added `kbIndexStart`, `kbReindexAll`, `kbSearch`
- `src/routes/Settings.tsx` — KB section updated: progress bar (listens to `kb-index-progress` event), "Reindex" button, folder picker auto-triggers indexing

### Tauri commands (M2)
- `kb_index_start(folder)` — ensures embed model inited, runs full index, starts watcher
- `kb_reindex_all()` — re-runs full index on configured folder
- `kb_search(query, top_k?)` → `Vec<SearchResult>` (chunk_id, file_path, breadcrumb, snippet, score)

### AppState (current)
```rust
pub struct AppState {
    pub pool: SqlitePool,
    pub data_dir: PathBuf,          // app_data_dir()
    pub embed_model: EmbedModel,    // Arc<Mutex<Option<TextEmbedding>>>
    pub watcher_task: Mutex<Option<JoinHandle<()>>>,
}
```

### Cargo deps added in M2
`fastembed = "4"`, `notify = "7"`, `sha2 = "0.10"`, `hex = "0.4"`, `walkdir = "2"`

---

## M3 — Audio capture + live Whisper + Live Session screen ✅

### Key files

**Rust:**
- `src-tauri/src/audio/mod.rs` — AudioSource enum, `resample_to_16k`, `to_mono`
- `src-tauri/src/audio/mic.rs` — cpal mic capture (F32/I16), `capture_loop` runs on std::thread
- `src-tauri/src/audio/system.rs` — Linux PulseAudio monitor source via cpal; gracefully skips if no monitor found
- `src-tauri/src/audio/vad.rs` — Energy-based VAD (threshold_on=0.015, threshold_off=0.008, redemption=1.5s, min=250ms, pre-roll=200ms)
- `src-tauri/src/stt/whisper.rs` — whisper-rs 0.16 engine; lazy load via `Arc<Mutex<Option<Arc<WhisperContext>>>>`; `transcribe()` runs in spawn_blocking; `download_model()` streams from HuggingFace with progress events
- `src-tauri/src/meeting/session.rs` — session lifecycle: creates meeting in DB, launches engine tokio task, WAV writer on dedicated thread (hound, 16kHz f32 mono), VAD per source, dispatches speech segments to whisper, emits `transcript-segment` events + writes to DB; `recover_interrupted()` called on startup

**Frontend:**
- `src/routes/LiveSession.tsx` — two states: SetupPanel (title input, model download, Start button) and RecordingPanel (transcript list + nudge placeholder + timer/Stop)
- `src/stores/session.ts` — Zustand store for recording state + transcript entries
- `src/components/Sidebar.tsx` — updated: shows "· Recording" pulsing dot or "+ New Meeting" link
- `src/App.tsx` — added `/live` route

### Tauri commands (M3)
- `audio_devices_list()` → `Vec<DeviceInfo>`
- `whisper_model_status()` → `WhisperStatus { ready, model_name, model_path }`
- `whisper_download_model(model_name)` → downloads to `app_data_dir/models/`, emits `whisper-download-progress`
- `meeting_start(title, platform?)` → creates DB record, loads model, starts engine, returns meeting_id
- `meeting_stop()` → signals stop, flushes VAD, updates DB to `status='processing'`, returns meeting_id

### AppState additions (M3)
```rust
pub whisper_model: Arc<Mutex<Option<Arc<WhisperContext>>>>,
pub active_session: tokio::sync::Mutex<Option<ActiveSession>>,
```

### Cargo deps added in M3
`cpal = "0.15"`, `hound = "3"`, `whisper-rs = "0.16"`, `uuid = { version = "1", features = ["v4"] }`

### Build notes
- `clang` must be installed: `sudo pacman -S clang` (needed by whisper-rs for bindgen)
- whisper-rs builds whisper.cpp from source via cmake — first compile ~5-10 min
- Energy VAD used instead of silero (no extra ONNX dep; acceptable for demo)
- System audio capture: PipeWire/PulseAudio monitor sources only. If none found, mic-only works.

### First use
1. Open app → click "+ New Meeting" in sidebar
2. Click "Download (~75 MB)" to fetch `ggml-tiny.en.bin` from HuggingFace
3. Enter title → "Start Meeting"
4. Speak → transcript appears within ~2-4 s per utterance
5. Click "Stop" → meeting persists in DB

---

## Environment

- OS: Arch Linux
- Rust: 1.95.0 via rustup (`~/.cargo/bin/`)
- Bun: 1.3.11
- Tauri Linux deps: webkit2gtk-4.1, gtk3, libappindicator-gtk3, librsvg, libsoup3, base-devel, openssl, pkgconf, libayatana-appindicator — all installed
- xdotool: NOT installed (only needed for X11 global shortcuts in M7)
- clang: installed (required by whisper-rs bindgen)
