# MeetAI — Full Implementation Walkthrough

**Project:** Final Year Design Project (FYDP)  
**Stack:** Tauri v2 · Rust · React 19 · TypeScript · Tailwind v4  
**Location:** `/home/hussain/Programming/fypmeet/fydp/meetaiapp/`

---

## What It Does

MeetAI is a privacy-first, cross-platform AI meeting assistant that runs entirely on your machine. During a meeting it:

1. Captures both your microphone and the other party's audio (system loopback)
2. Transcribes both streams in real time using Whisper (local, CPU)
3. Searches your personal knowledge base for relevant notes and surfaces them as nudge cards
4. Optionally generates an AI talking point per nudge via Groq
5. After the meeting: embeds the transcript, generates a structured summary (decisions, action items, topics) via Groq, and exports to Markdown/Obsidian

All data stays in a local SQLite database. The only network calls are to Groq's API for language model inference.

---

## Hard Constraints (Never Broken)

| Constraint | Reason |
|---|---|
| Groq is the only LLM provider | `llama-3.1-8b-instant` (live/nudge), `llama-3.3-70b-versatile` (summaries) |
| All data in local SQLite | Privacy — nothing leaves the machine except Groq API calls |
| All embeddings local via fastembed | No embedding API calls; `bge-small-en-v1.5` (384-dim, ~30 MB, CPU) |
| Tauri v2 only | Cross-platform desktop shell |
| `bun` for all JS operations | Not npm, not pnpm |

---

## Repository Layout

```
fydp/
├── design/meetai-ui.html          # M0: static design reference (all 5 screens)
├── meetaiapp/                     # The Tauri application
│   ├── index.html                 # Main window HTML entry
│   ├── overlay.html               # Overlay window HTML entry (M7)
│   ├── vite.config.ts             # MPA build (main + overlay)
│   ├── package.json               # bun deps
│   ├── src/                       # React frontend
│   │   ├── App.tsx                # Root router + meeting-detected banner
│   │   ├── App.css                # Design tokens, base styles
│   │   ├── ipc/index.ts           # All typed invoke() wrappers
│   │   ├── stores/
│   │   │   ├── session.ts         # Zustand: recording state, transcript, nudge cards
│   │   │   └── settings.ts        # Zustand: settings mirror
│   │   ├── components/Sidebar.tsx # Navigation sidebar
│   │   ├── routes/
│   │   │   ├── Onboarding.tsx     # M8: 3-step first-launch wizard
│   │   │   ├── Settings.tsx       # Settings screen (6 sections + About)
│   │   │   ├── Library.tsx        # Meeting list, search, grouped by day
│   │   │   ├── Chat.tsx           # Chat with meetings (RAG)
│   │   │   ├── LiveSession.tsx    # Setup panel + recording panel + nudge cards
│   │   │   └── PostMeeting.tsx    # 4-tab post-meeting view
│   │   └── overlay/
│   │       ├── main.tsx           # Overlay window entry point
│   │       └── Overlay.tsx        # Compact floating overlay component
│   └── src-tauri/
│       ├── Cargo.toml             # Rust dependencies
│       ├── tauri.conf.json        # Tauri config (windows, bundle, security)
│       ├── capabilities/
│       │   ├── default.json       # Permissions for main window
│       │   └── overlay.json       # Permissions for overlay window
│       ├── migrations/
│       │   └── 0001_init.sql      # Full DB schema
│       └── src/
│           ├── lib.rs             # AppState, all Tauri commands, setup()
│           ├── bin/
│           │   └── meetai_mcp.rs  # M10: standalone stdio MCP server binary
│           ├── db/mod.rs          # SQLite pool init, migrations, WAL mode
│           ├── settings/mod.rs    # Settings CRUD, Groq connection test
│           ├── audio/
│           │   ├── mod.rs         # AudioSource enum, resampler, mono converter
│           │   ├── mic.rs         # cpal mic capture loop
│           │   ├── system.rs      # System audio: cpal monitor → parec fallback
│           │   ├── vad.rs         # Energy-based VAD with pre-roll buffer
│           │   └── autodetect.rs  # M8: RMS polling for auto-start detection
│           ├── stt/
│           │   ├── mod.rs         # Re-exports, WhisperStatus, WhisperModel type alias
│           │   └── whisper.rs     # whisper-rs 0.16 engine, model download
│           ├── kb/
│           │   ├── mod.rs         # Re-exports
│           │   ├── chunker.rs     # Markdown-aware chunker (80–500 words, 20% overlap)
│           │   ├── embed.rs       # fastembed bge-small-en-v1.5, lazy init
│           │   ├── index.rs       # SHA256 skip-unchanged, batch embed, progress events
│           │   ├── search.rs      # Brute-force cosine similarity
│           │   └── watcher.rs     # notify crate, 500ms debounce, re-index on change
│           ├── llm/mod.rs         # Non-streaming Groq chat (OpenAI-compatible)
│           ├── nudge/mod.rs       # Nudge engine: 100ms tick, KB search, Jaccard dedup
│           └── meeting/
│               ├── mod.rs         # Re-exports, MeetingRow, MeetingDetail types
│               ├── session.rs     # Recording lifecycle: threads, VAD→Whisper, WAV writer
│               ├── library.rs     # DB queries: list, search, get, chat context, export
│               └── jobs.rs        # Post-meeting job queue: Embed + Summarize
```

---

## Database Schema (`0001_init.sql`)

```sql
-- Core tables
meetings          (id TEXT PK, title, platform, status, started_at, ended_at, duration_ms, notes, audio_path, tags)
transcript_segments (id INT PK, meeting_id FK, source, speaker_id, speaker_name, text, start_ms, end_ms, is_final)
summaries         (id INT PK, meeting_id FK UNIQUE, overview, decisions JSON, topics JSON)
action_items      (id INT PK, meeting_id FK, text, assignee, due_date, done INT)

-- Settings
settings          (key TEXT PK, value TEXT)

-- Knowledge base
kb_files          (id INT PK, path UNIQUE, sha256, indexed_at)
kb_chunks         (id INT PK, file_id FK, chunk_index, text, breadcrumb, embedding BLOB)

-- Post-meeting embeddings
meeting_chunks    (id INT PK, meeting_id FK, chunk_index, text, start_ms, end_ms, embedding BLOB)

-- Jobs
jobs              (id INT PK, meeting_id FK, kind, status, error, started_at, finished_at)

-- FTS virtual tables
segments_fts      USING fts5(text, content=transcript_segments)
meetings_fts      USING fts5(title, content=meetings)

-- Pre-meeting context
pre_context_chunks (id INT PK, meeting_id FK, text, embedding BLOB)
```

All embeddings stored as raw 384-dim f32 little-endian BLOBs. Cosine similarity computed in Rust (brute force — fine up to ~50k chunks). No sqlite-vec extension required.

---

## AppState (Rust)

```rust
pub struct AppState {
    pub pool: SqlitePool,                               // shared DB pool
    pub data_dir: PathBuf,                              // ~/.local/share/com.hussain.meetai/
    pub embed_model: Arc<Mutex<Option<TextEmbedding>>>, // lazy — loads on first KB op
    pub watcher_task: Mutex<Option<JoinHandle<()>>>,    // notify watcher handle
    pub whisper_model: Arc<Mutex<Option<Arc<WhisperContext>>>>, // lazy — loads on meeting_start
    pub active_session: Mutex<Option<ActiveSession>>,   // Some() while recording
    pub job_tx: mpsc::Sender<JobRequest>,               // post-meeting pipeline queue
    pub auto_detect: Mutex<Option<AutoDetectHandle>>,   // M8: audio RMS watcher
}
```

State is initialized in `setup()` inside `tauri::async_runtime::block_on`. Models are `Arc<Mutex<Option<…>>>` so they are lazily initialized on first use and shared across threads without blocking setup.

---

## Milestone Implementations

### M0 — Design HTML

`design/meetai-ui.html` — single static file showing all five screens stacked vertically:
Settings, Library, Live Session, Live Overlay, Post-Meeting.

**Design tokens:**
```css
--paper: #f5f2ec;   /* warm off-white background */
--ink: #0d0d0f;     /* near-black text */
--amber: #e8a445;   /* nudge cards, warnings */
--green: #2d7a5a;   /* success, speaker labels */
--blue: #2a5fac;    /* "You" labels, links */
--red: #c83c3c;     /* recording indicator, errors */
```
**Fonts:** Instrument Serif (headings), DM Mono (labels/code), Geist (UI body).

---

### M1 — Scaffold + DB + Settings

**Scaffold:** Tauri v2 + React 19 + Vite + TypeScript + Tailwind v4. State management: Zustand. Routing: React Router v7 (MemoryRouter, 5 routes).

**DB init (`db/mod.rs`):**
```rust
SqliteConnectOptions::from_str(&url)?
    .create_if_missing(true)
    .journal_mode(SqliteJournalMode::Wal)   // WAL enables concurrent reads
    .foreign_keys(true)
```
Uses sqlx 0.8 non-macro queries (`sqlx::query().bind()`) — no `DATABASE_URL` compile-time requirement.

**Settings (`settings/mod.rs`):** Key-value table. `settings_get` reads all rows and populates a `Settings` struct with defaults. `settings_set` upserts a single key. `groq_test_connection` hits Groq's `/openai/v1/models` endpoint.

**Frontend settings store (`stores/settings.ts`):** Zustand store that mirrors the Rust struct. Debounced 400ms writes on input change; Settings screen reads from store to avoid re-fetching.

---

### M2 — KB Ingestion + Embeddings + Watcher

**Chunker (`kb/chunker.rs`):**
- Markdown-aware: respects heading boundaries, code blocks, paragraphs
- Target: 80–500 words per chunk, ~20% overlap between consecutive chunks
- Each chunk carries a `breadcrumb` (e.g. `"filename > # Heading > ## Subheading"`)
- Port of the algorithm from OpenOats/KnowledgeBase.swift:547

**Embedder (`kb/embed.rs`):**
```rust
pub type EmbedModel = Arc<Mutex<Option<TextEmbedding>>>;
```
fastembed v4 with `bge-small-en-v1.5` (384 dimensions, ~30 MB, CPU ONNX). Lazy init: first call downloads the model to `data_dir/models/`, subsequent calls reuse it. Runs in `spawn_blocking` to avoid blocking the tokio runtime.

**Indexer (`kb/index.rs`):**
1. Walk folder with `walkdir`
2. For each file: SHA256 → compare with `kb_files.sha256` → skip if unchanged
3. Read text → chunk → embed batch → store BLOBs in `kb_chunks`
4. Emit `kb-index-progress` events to frontend

**Search (`kb/search.rs`):**
Brute-force cosine similarity. Load all embeddings from DB, deserialize f32 BLOBs, compute dot product (vectors are L2-normalized by fastembed). Returns top-K with score, breadcrumb, snippet.

**Watcher (`kb/watcher.rs`):**
`notify` crate with a `RecommendedWatcher`. Events bridged from a `std::thread` to tokio via `std::sync::mpsc` → `tokio::sync::mpsc`. 500ms debounce before triggering incremental reindex.

---

### M3 — Audio Capture + Live Whisper

**Audio pipeline overview:**
```
mic thread ──────────────────────────────────────────────┐
                                                         ▼
                                              tokio mpsc channel
system thread (parec) ───────────────────────────────────┘
                                                         │
                                              engine tokio task
                                                         │
                                       ┌─────────────────┤
                                       ▼                 ▼
                                  VAD (per source)   WAV writer thread
                                       │             (hound, 16kHz f32)
                               speech segment
                                       │
                                  Whisper (spawn_blocking)
                                       │
                                  transcript-segment event + DB write
```

**Mic capture (`audio/mic.rs`):**
- cpal 0.15, default input device
- Handles F32 and I16 sample formats
- Converts to mono (`to_mono`), sends `(AudioSource::Mic, Vec<f32>, sample_rate)` via `try_send` (drops if full — never blocks the audio callback)

**System audio (`audio/system.rs`) — two paths:**

*Primary — cpal ALSA monitor:* Scans `host.input_devices()` for a name containing "monitor". Works on classic PulseAudio where the monitor is exposed as an ALSA PCM device.

*Fallback — `parec` subprocess (active on PipeWire/Arch Linux):*
```rust
// 1. Find active output sink via `pactl info`
let sink = pactl_info()  // "Default Sink: bluez_output.XX.1"
// 2. Derive monitor: "bluez_output.XX.1.monitor"
// 3. Spawn parec:
Command::new("parec")
    .args(["--device", &monitor,
           "--format=float32le", "--rate=16000",
           "--channels=1", "--latency-msec=50"])
    .stdout(Stdio::piped())
```
Reads raw f32le bytes from stdout in 4096-byte chunks (1024 samples = 64ms). Already at 16kHz, so no resampling needed. Correctly follows the active output device — works for Bluetooth, HDMI, built-in, etc.

**Resampler (`audio/mod.rs`):**
Linear interpolation to 16 kHz. No external crate (avoids rubato dependency).
```rust
let ratio = from_rate as f64 / 16000.0;
// for each output sample i: interpolate between samples[floor(i*ratio)] and [ceil]
```

**VAD (`audio/vad.rs`):**
Energy-based (no Silero ONNX — avoids ort complexity for live use):
```
threshold_on  = 0.015 RMS  → start speech segment
threshold_off = 0.008 RMS  → end speech segment (after redemption period)
redemption    = 1.5 s      → stay in "speech" this long after energy drops
min_speech    = 250 ms     → discard shorter segments
pre_roll      = 200 ms     → include audio before voice onset
```
`VecDeque<f32>` pre-roll buffer preserves the audio just before each utterance.

**Whisper (`stt/whisper.rs`):**
whisper-rs 0.16 (NOT 0.13 — the bundled whisper.cpp C API changed; 0.13 has field mismatches).

Key 0.16 API differences vs 0.13:
- `state.full_n_segments()` returns `i32` directly (not `Result<i32>`)
- `full_get_segment_text(i)` renamed to `get_segment(i)` returning `Option<WhisperSegment>`; text via `seg.to_str()`

Model download: HTTP GET from HuggingFace with streaming progress events (`whisper-download-progress`). Stored in `data_dir/models/ggml-{name}.bin`.

**Session lifecycle (`meeting/session.rs`):**
```
meeting_start()
  → load Whisper model (ensure_loaded)
  → INSERT meeting row (status=recording)
  → spawn mic std::thread
  → spawn system std::thread  
  → spawn WAV writer std::thread (hound WavWriter, 16kHz f32 mono)
  → spawn engine tokio task:
      loop { recv audio chunk → VAD → if speech → transcribe → emit event + DB write }
  → start nudge engine (optional)
  → set content_protected on both windows (if setting enabled)
  → return meeting_id

meeting_stop()
  → set stop AtomicBool
  → join engine task
  → calculate duration_ms
  → UPDATE meeting (status=processing, ended_at, duration_ms)
  → enqueue Embed + Summarize jobs
  → remove content_protected
  → hide overlay
```

`recover_interrupted()` runs at startup: any meeting still in `status='recording'` is set to `status='processing'` (data already in DB, just never got the stop signal).

---

### M4 — KB Nudge Loop

**Nudge engine (`nudge/mod.rs`), runs as a tokio task:**

```
every 100ms tick:
  1. Query last 40 words from transcript_segments (most recent final segments)
  2. Embed that text via fastembed (spawn_blocking)
  3. Cosine search against kb_chunks → top-5
  4. For each result above threshold (default 0.65):
     a. Jaccard similarity vs last 3 emitted cards (token overlap)
        → skip if >0.7 similarity (deduplicate near-duplicate cards)
     b. If passes gate → optionally call Groq 8b-instant for a talking point
     c. Emit "nudge-update" event with { id, file_path, breadcrumb, snippet, score, suggestion }
     d. Store in last-3 ring buffer
  5. Wait for next tick
```

**Jaccard dedup:**
```rust
fn jaccard(a: &str, b: &str) -> f32 {
    let set_a: HashSet<&str> = a.split_whitespace().collect();
    let set_b: HashSet<&str> = b.split_whitespace().collect();
    let intersection = set_a.intersection(&set_b).count();
    intersection as f32 / (set_a.len() + set_b.len() - intersection) as f32
}
```

**Frontend nudge panel (`LiveSession.tsx`):**
Zustand store holds `nudgeCards: NudgeCard[]` (max 3, newest first). Rendered with opacity 1.0 / 0.6 / 0.3 for most-recent to oldest.

---

### M5 — Library + Chat

**Library screen (`Library.tsx`):**
- Groups meetings by calendar day (local time)
- 300ms debounced LIKE search: calls `meeting_search(query)` which does `WHERE title LIKE ? OR id IN (SELECT meeting_id FROM transcript_segments WHERE text LIKE ?)`
- Status badges: recording (red) / processing (amber) / done (green) / error (red)
- Click → navigate to `/meeting/:id` (or `/live` if still recording)

**Chat (`Chat.tsx`):**
`chat_query(question)` command:
1. LIKE search across transcript segments for the question text
2. Format matching segments as `[Meeting: {title}]\n[mm:ss] {Speaker}: {text}`
3. Send to Groq 8b-instant with system prompt: "Answer using ONLY these transcript excerpts"
4. Return `{ answer, sources: [{id, title, started_at}] }`

Frontend renders citations as clickable tags that navigate to the meeting.

---

### M6 — Post-Meeting Pipeline

**Job queue (`meeting/jobs.rs`):**
Single tokio mpsc channel (buffer 64). One consumer task processes jobs sequentially. Each job has a row in the `jobs` table tracking status/error/timing.

**Embed job:**
1. Fetch all final transcript segments for the meeting
2. Chunk into ~200-word windows with speaker labels: `"You: ... Speaker: ..."`
3. Embed via fastembed
4. Write BLOBs to `meeting_chunks`
5. These enable semantic search across meeting content

**Summarize job:**
```rust
let prompt = format!(
    "You are a meeting analyst. Analyze this transcript and respond with ONLY valid JSON:\n\
     {{\"overview\": \"...\", \"decisions\": [...], \"action_items\": [...], \"topics\": [...]}}\n\n\
     Transcript:\n{full_transcript}"
);
// → llama-3.3-70b-versatile via Groq (30s timeout)
// → strip markdown fences (```json ... ```)
// → parse JSON
// → INSERT into summaries + action_items tables
// → UPDATE meetings SET status='done'
// → if obsidian_vault configured → write Markdown file with YAML frontmatter
```

**Post-Meeting screen (`PostMeeting.tsx`):**
Four tabs:
- **Summary:** job progress badges, overview text, decisions list, action items with checkboxes (optimistic toggle), topic chips, Regenerate button
- **Transcript:** speaker-coloured rows (blue=You, green=Speaker) with mm:ss timestamps
- **Notes:** textarea, auto-saves 600ms after last keystroke
- **Export:** client-side Blob download (.md) + copy to clipboard

Refreshes automatically on `job-progress` events for this meeting ID.

---

### M7 — Live Overlay Window

**Overlay window creation (in `setup()`):**
```rust
let url = if cfg!(debug_assertions) {
    tauri::WebviewUrl::External("http://localhost:1420/overlay.html".parse().unwrap())
} else {
    tauri::WebviewUrl::App("overlay.html".into())
};
WebviewWindowBuilder::new(app, "overlay", url)
    .decorations(false).transparent(true).always_on_top(true)
    .skip_taskbar(true).inner_size(380.0, 220.0)
    .position(1540.0, 860.0).visible(false)
    .build()?
```

Created at startup (hidden). Shown/hidden via `overlay_show` / `overlay_hide` / `overlay_toggle` commands, or the `Ctrl+Shift+O` global shortcut.

**Global shortcut:**
Registered via `tauri-plugin-global-shortcut`. Handler is set in `Builder::with_handler()` (fires for all registered shortcuts). Actual shortcut registered in `setup()` via `app.handle().global_shortcut().register(...)`. On Linux with Wayland the registration may silently fail — documented in Settings.

**Content protection (`set_content_protected`):**
Called on both windows when a recording starts (if setting enabled). No-op on Linux; on macOS sets `NSWindow.sharingType = .none`; on Windows sets `WDA_EXCLUDEFROMCAPTURE`.

**Overlay UI (`overlay/Overlay.tsx`):**
- Last 3 final transcript segments with fading opacity (0.45 → 0.73 → 1.0 newest)
- Current nudge card with amber border, breadcrumb, suggestion text
- "Expand ↗" button: hides overlay, focuses main window via `WebviewWindow.getByLabel("main")`
- "Stop" button: calls `meeting_stop`, hides overlay

**Vite MPA:** `vite.config.ts` adds `build.rollupOptions.input` with both `index.html` and `overlay.html`. Dev server serves `overlay.html` at `http://localhost:1420/overlay.html` automatically.

---

### M8 — Onboarding + Auto-Detect

**Onboarding (`App.tsx` + `routes/Onboarding.tsx`):**

`App.tsx` checks `groq_key` on mount before rendering the router:
```tsx
ipc.settingsGet().then(s =>
    setInitialRoute(s.groq_key.trim() ? "/library" : "/onboarding")
)
```
`MemoryRouter` is constructed with the resolved initial route, so the first render is always correct.

3-step wizard:
1. **Welcome** — feature overview (transcript, nudges, summaries)
2. **Groq key** — password input + Test button + link to `console.groq.com/keys` (opens via `@tauri-apps/plugin-opener`)
3. **KB folder** — optional folder picker; "Finish" starts indexing and navigates to Library

**Auto-detect (`audio/autodetect.rs`):**
Runs as a `std::thread`. Every 2 seconds, computes RMS of the last 2 seconds of audio from the default mic:
```
RMS > 0.018 for 10 consecutive seconds → emit "meeting-detected" event
RMS drops below threshold → reset counter (allows re-trigger)
```

`meeting-detected` event caught in `App.tsx` → shows `DetectedBanner` (fixed top bar with title input + Start button → navigates to `/live`).

Auto-detect thread started in `setup()` if `auto_start` setting was previously enabled. `auto_start_enable` / `auto_start_disable` commands start/stop it at runtime (wired to the Settings toggle).

---

### M9 — Polish: Cold Start, Memory, Telemetry

**Crash log:**
```rust
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

// In run(), before Builder:
std::panic::set_hook(Box::new(|info| {
    if let Some(path) = LOG_PATH.get() {
        // append "[unix_ts] [PANIC] {info}\n" to log file
    }
}));

// In setup(), after data_dir is known:
simplelog::WriteLogger::init(LevelFilter::Warn, Config::default(), file).ok();
LOG_PATH.set(log_path).ok();
```

Log file: `~/.local/share/com.hussain.meetai/logs/meetai.log`

**Cold start audit (all lazy by design):**

| What | When |
|---|---|
| DB open + migrations | At startup (fast — few rows) |
| Recover interrupted meetings | At startup (single query) |
| fastembed model | First KB index/search op |
| Whisper model | First `meeting_start` |
| Auto-detect thread | At startup only if `auto_start = true` |
| Groq API calls | Never at startup |

**Memory characteristics:**
- Audio: ring-buffered 50ms chunks, `try_send` drops frames if the engine lags
- Transcript segments: written to SQLite (disk), not RAM
- WAV file: streamed to disk via `hound::WavWriter` — no in-memory accumulation
- Nudge cards: max 3 in Zustand store
- Whisper model: ~130 MB RSS when loaded

**Settings › Version & logs:** Shows build info, log file path with "Open" button (`openPath` via plugin-opener), cold-start breakdown, memory characteristics.

---

### M10 — MCP Server

**Binary (`src-tauri/src/bin/meetai_mcp.rs`):**
Standalone Rust binary — no Tauri dependency. Shares Cargo.toml (uses sqlx, serde_json, tokio, anyhow). Registered as `default-run = "meetai"` in Cargo.toml so `cargo run` still picks the main app.

**Protocol:** MCP v2024-11-05 over stdio, newline-delimited JSON-RPC 2.0.

```
stdin  → {"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}
stdout ← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05",...}}

stdin  → {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
stdout ← {"jsonrpc":"2.0","id":2,"result":{"tools":[...]}}

stdin  → {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_meetings","arguments":{"query":"pricing"}}}
stdout ← {"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"Found 2..."}]}}
```

Notifications (no `id`) are silently ignored.

**Four tools:**

| Tool | SQL | Returns |
|---|---|---|
| `search_meetings(query)` | LIKE on title + transcript_segments.text | Meeting list with metadata |
| `get_meeting_transcript(meeting_id)` | All final segments ordered by start_ms | Speaker-labelled transcript |
| `get_action_items(meeting_id?)` | All pending, or filtered to one meeting | Checkbox list |
| `get_recent_meetings(n?)` | Latest N with LEFT JOIN summaries | Titles + overviews |

**DB path:** `--db /path/to/meetai.db` argument. Fallback: `$XDG_DATA_HOME/com.hussain.meetai/meetai.db`. Opened read-only (`SqliteConnectOptions::read_only(true)`) — safe to run alongside the main app thanks to WAL mode.

**Usage:**
```bash
# Build
cargo build --release --bin meetai_mcp

# Settings → Integrations → enable MCP → copy config snippet
# Paste into ~/.config/Claude/claude_desktop_config.json
{
  "mcpServers": {
    "meetai": {
      "command": "/path/to/meetai_mcp",
      "args": ["--db", "/home/user/.local/share/com.hussain.meetai/meetai.db"]
    }
  }
}
```

---

## All Tauri Commands

| Command | Returns | Milestone |
|---|---|---|
| `settings_get` | `Settings` | M1 |
| `settings_set(key, value)` | `()` | M1 |
| `groq_test_connection(key)` | `bool` | M1 |
| `kb_index_start(folder)` | `()` | M2 |
| `kb_reindex_all()` | `()` | M2 |
| `kb_search(query, top_k?)` | `Vec<SearchResult>` | M2 |
| `audio_devices_list()` | `Vec<DeviceInfo>` | M3 |
| `whisper_model_status()` | `WhisperStatus` | M3 |
| `whisper_download_model(model_name)` | `()` | M3 |
| `meeting_start(title, platform?)` | `String` (meeting_id) | M3 |
| `meeting_stop()` | `String` (meeting_id) | M3 |
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

## All Tauri Events (Rust → Frontend)

| Event | Payload | Source |
|---|---|---|
| `kb-index-progress` | `{current, total, file, done}` | KB indexer |
| `whisper-download-progress` | `{model, downloaded, total, percent, done}` | STT |
| `transcript-segment` | `{meeting_id, source, text, start_ms, end_ms, is_final}` | Audio engine |
| `nudge-update` | `{id, file_path, breadcrumb, snippet, score, suggestion?}` | Nudge engine |
| `job-progress` | `{meeting_id, kind, status, error?}` | Job queue |
| `obsidian-export-done` | `{meeting_id}` | Summarize job |
| `meeting-detected` | `()` | Auto-detect |

---

## Key Design Decisions

**Why energy VAD instead of Silero?**
Silero requires the `ort` crate (ONNX runtime), which adds significant compile-time complexity and a large native library. Energy VAD with tuned thresholds (threshold_on=0.015, redemption=1.5s) gives acceptable results for meeting audio and compiles in seconds.

**Why `parec` subprocess instead of PulseAudio/PipeWire native API?**
cpal's ALSA backend doesn't see PipeWire monitor sources — they live in the PulseAudio namespace. Using `parec` (part of `pipewire-pulse`) is reliable, zero extra dependencies, and correctly follows the active output device via `pactl info → Default Sink`.

**Why non-streaming Groq calls?**
Streaming SSE requires `futures-util` and more complex state management for partial tokens. For nudge synthesis (12s timeout) and chat (30s timeout), non-streaming is simpler and the latency difference is not user-perceptible. Summary generation (post-meeting, not interactive) clearly benefits from simplicity over streaming.

**Why brute-force cosine similarity instead of sqlite-vec?**
sqlite-vec requires bundling a native extension per platform and loading it at connection time. Brute-force over 384-dim BLOBs in Rust is fast enough for up to ~50k chunks (typical personal KB). Removed a major distribution complexity.

**Why sequential job queue (not parallel)?**
Embed + Summarize both use CPU-heavy operations (fastembed, Groq network). Running them in parallel would thrash CPU during the post-meeting window. Sequential processing keeps the system responsive immediately after a meeting.

**Why `default-run = "meetai"` in Cargo.toml?**
Adding the `meetai_mcp` binary caused `cargo run` ambiguity — Tauri's dev command uses `cargo run` internally. `default-run` tells Cargo which binary is the "main" one.

---

## How to Run

```bash
cd /home/hussain/Programming/fypmeet/fydp/meetaiapp

# Dev (incremental after first build)
bun tauri dev

# First build takes ~10 min (whisper.cpp C++ compilation)
# Subsequent incremental builds: ~5–30 seconds depending on what changed

# Prerequisites
sudo pacman -S clang cmake   # required by whisper-rs bindgen + build
# webkit2gtk-4.1, gtk3, libsoup3, etc. already installed
```

**First-run checklist:**
1. App opens → Onboarding wizard (if no Groq key stored)
2. Enter Groq key → Test → Next
3. Optionally pick KB folder → Finish (embed model downloads ~30 MB)
4. Settings → Recording → Download Whisper model (~75 MB, tiny.en)
5. "+ New Meeting" → start talking

---

## Build MCP Binary

```bash
cargo build --release --bin meetai_mcp
# Binary at: target/release/meetai_mcp

# Enable in: Settings → Integrations → MCP server toggle
# Copy the config snippet shown → paste into ~/.config/Claude/claude_desktop_config.json
# Restart Claude Desktop
```

---

## Environment

| Tool | Version |
|---|---|
| OS | Arch Linux |
| Rust | 1.95.0 (rustup) |
| Bun | 1.3.11 |
| clang | system (pacman) |
| cmake | 4.3.x |
| Tauri CLI | 2.x |
| Node (via bun) | bundled |
