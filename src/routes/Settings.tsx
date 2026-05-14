import { useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { ipc, type Settings } from "../ipc";
import { useSettingsStore } from "../stores/settings";
import Sidebar from "../components/Sidebar";

type Section =
  | "connection"
  | "kb"
  | "recording"
  | "nudges"
  | "privacy"
  | "integrations"
  | "about";

export default function Settings() {
  const { settings, loaded, load, set } = useSettingsStore();
  const [activeSection, setActiveSection] = useState<Section>("connection");
  const [groqStatus, setGroqStatus] = useState<"idle" | "testing" | "ok" | "error">("idle");
  const [syncLabel, setSyncLabel] = useState("Synced · just now");
  const [kbIndexing, setKbIndexing] = useState(false);
  const [kbProgress, setKbProgress] = useState<{ current: number; total: number; file: string } | null>(null);
  const debounceRef = useRef<Record<string, ReturnType<typeof setTimeout>>>({});

  useEffect(() => {
    load();
    const unlisten = listen<{ current: number; total: number; file: string; done: boolean }>(
      "kb-index-progress",
      (e) => {
        if (e.payload.done) {
          setKbIndexing(false);
          setKbProgress(null);
        } else {
          setKbIndexing(true);
          setKbProgress(e.payload);
        }
      }
    );
    return () => { unlisten.then((f) => f()); };
  }, []);

  if (!loaded || !settings) {
    return (
      <div className="flex h-screen items-center justify-center text-[var(--muted)] text-sm">
        Loading…
      </div>
    );
  }

  const persist = (key: keyof Settings, value: string) => {
    clearTimeout(debounceRef.current[key]);
    debounceRef.current[key] = setTimeout(async () => {
      await set(key, value);
      setSyncLabel("Synced · just now");
    }, 400);
  };

  const handleGroqTest = async () => {
    setGroqStatus("testing");
    try {
      const ok = await ipc.groqTestConnection(settings.groq_key);
      setGroqStatus(ok ? "ok" : "error");
    } catch {
      setGroqStatus("error");
    }
  };

  const handleKbChoose = async () => {
    const path = await openDialog({ directory: true, multiple: false });
    if (typeof path === "string") {
      await set("kb_folder", path);
      setKbIndexing(true);
      ipc.kbIndexStart(path).catch(() => setKbIndexing(false));
    }
  };

  const handleReindex = async () => {
    if (!settings?.kb_folder) return;
    setKbIndexing(true);
    ipc.kbReindexAll().catch(() => setKbIndexing(false));
  };

  const tocItems: { id: Section; label: string }[] = [
    { id: "connection", label: "Connection" },
    { id: "kb", label: "Knowledge base" },
    { id: "recording", label: "Recording" },
    { id: "nudges", label: "Nudges & AI" },
    { id: "privacy", label: "Privacy" },
    { id: "integrations", label: "Integrations" },
  ];

  return (
    <div className="flex h-screen overflow-hidden bg-[var(--paper)]">
      <Sidebar groqOk={groqStatus === "ok" ? true : groqStatus === "error" ? false : null} />

      <div className="flex flex-col flex-1 overflow-hidden">
        {/* Header */}
        <div className="flex items-end justify-between px-8 py-5 border-b border-[var(--border)] bg-[var(--surface)] shrink-0">
          <div>
            <div
              className="text-[10px] tracking-widest uppercase mb-1"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              Configure
            </div>
            <h2
              className="text-2xl font-normal m-0"
              style={{ fontFamily: "var(--font-serif)" }}
            >
              Settings <em style={{ color: "var(--muted)" }}>· preferences</em>
            </h2>
          </div>
          <span
            className="text-[11px] px-2.5 py-1 rounded border border-[var(--border)]"
            style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
          >
            {syncLabel}
          </span>
        </div>

        <div className="flex flex-1 overflow-hidden">
          {/* TOC */}
          <aside className="w-44 shrink-0 py-6 px-4 border-r border-[var(--border)] flex flex-col gap-0.5 overflow-y-auto">
            {tocItems.map((item) => (
              <button
                key={item.id}
                onClick={() => setActiveSection(item.id)}
                className={`text-left px-3 py-1.5 rounded text-sm border-none cursor-pointer transition-colors ${
                  activeSection === item.id
                    ? "bg-[var(--surface-subtle)] font-medium text-[var(--ink)]"
                    : "bg-transparent text-[var(--muted)] hover:text-[var(--ink)]"
                }`}
              >
                {item.label}
              </button>
            ))}
            <div
              className="mt-3 text-[10px] tracking-widest uppercase px-3 pt-2 border-t border-[var(--border)]"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              About
            </div>
            <button
              className={`text-left px-3 py-1.5 rounded text-sm border-none cursor-pointer transition-colors ${
                activeSection === "about"
                  ? "bg-[var(--surface-subtle)] font-medium text-[var(--ink)]"
                  : "bg-transparent text-[var(--muted)] hover:text-[var(--ink)]"
              }`}
              onClick={() => setActiveSection("about")}
            >
              Version & logs
            </button>
          </aside>

          {/* Content */}
          <main className="flex-1 overflow-y-auto px-8 py-8 max-w-2xl">
            {/* Connection */}
            {activeSection === "connection" && (
              <Section title="Connection" blurb="MeetAI uses Groq for all language model calls. Your key never leaves this machine except to call api.groq.com.">
                <Field label="Groq API key" help="Paste a key from console.groq.com/keys — the free tier is enough.">
                  <div className="flex flex-col gap-2">
                    <div className="flex gap-2">
                      <input
                        type="password"
                        className="input flex-1"
                        value={settings.groq_key}
                        placeholder="gsk_…"
                        onChange={(e) => {
                          set("groq_key", e.target.value);
                          persist("groq_key", e.target.value);
                        }}
                        style={{ fontFamily: "var(--font-mono)" }}
                      />
                      <button
                        className="btn"
                        onClick={handleGroqTest}
                        disabled={groqStatus === "testing"}
                      >
                        {groqStatus === "testing" ? "Testing…" : "Test"}
                      </button>
                    </div>
                    {groqStatus === "ok" && (
                      <StatusBadge ok>Connected · reachable</StatusBadge>
                    )}
                    {groqStatus === "error" && (
                      <StatusBadge ok={false}>Connection failed</StatusBadge>
                    )}
                  </div>
                </Field>

                <Field label="Models" help="Roles are fixed: an instant model for live nudges, a versatile model for summaries.">
                  <div className="flex flex-col gap-1.5">
                    <div className="flex items-center gap-3">
                      <span className="badge info">Live</span>
                      <span className="text-sm" style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}>
                        llama-3.1-8b-instant
                      </span>
                    </div>
                    <div className="flex items-center gap-3">
                      <span className="badge info">Post-meeting</span>
                      <span className="text-sm" style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}>
                        llama-3.3-70b-versatile
                      </span>
                    </div>
                  </div>
                </Field>
              </Section>
            )}

            {/* Knowledge base */}
            {activeSection === "kb" && (
              <Section title="Knowledge base" blurb="Point MeetAI at a folder of markdown or text files. They stay on disk; only embeddings are stored in the local vector DB.">
                <Field label="Folder" help="Watched live. New or edited files reindex within a second.">
                  <div className="flex flex-col gap-2">
                    <div className="flex gap-2">
                      <input
                        className="input flex-1"
                        value={settings.kb_folder}
                        placeholder="~/Documents/Vault"
                        readOnly
                        style={{ fontFamily: "var(--font-mono)" }}
                      />
                      <button className="btn" onClick={handleKbChoose} disabled={kbIndexing}>
                        Choose…
                      </button>
                      <button className="btn" onClick={handleReindex} disabled={kbIndexing || !settings.kb_folder}>
                        {kbIndexing ? "Indexing…" : "Reindex"}
                      </button>
                    </div>
                    {kbIndexing && kbProgress && (
                      <div className="flex flex-col gap-1">
                        <div className="h-1 bg-[var(--border)] rounded-full overflow-hidden">
                          <div
                            className="h-full bg-[var(--ink)] rounded-full transition-all"
                            style={{ width: `${kbProgress.total > 0 ? Math.round((kbProgress.current / kbProgress.total) * 100) : 0}%` }}
                          />
                        </div>
                        <div className="text-[11px]" style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}>
                          {kbProgress.current}/{kbProgress.total} · {kbProgress.file}
                        </div>
                      </div>
                    )}
                    {kbIndexing && !kbProgress && (
                      <div className="text-[11px]" style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}>
                        Initialising model…
                      </div>
                    )}
                  </div>
                </Field>

                <Field label="Embedding model" help="Local, CPU. Downloaded on first use (~30 MB).">
                  <div className="flex items-center gap-3">
                    <span className="badge ok">bge-small-en-v1.5</span>
                    <span className="text-sm" style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}>
                      384 dim
                    </span>
                  </div>
                </Field>
              </Section>
            )}

            {/* Recording */}
            {activeSection === "recording" && (
              <Section title="Recording" blurb="Live transcription runs locally. Post-meeting accuracy can optionally use the Parakeet sidecar.">
                <Field label="Live transcription model" help="Smaller is faster; larger is more accurate.">
                  <select
                    className="select"
                    value={settings.whisper_model}
                    onChange={(e) => {
                      set("whisper_model", e.target.value);
                      persist("whisper_model", e.target.value);
                    }}
                  >
                    <option value="whisper-tiny.en">whisper-tiny.en (fastest)</option>
                    <option value="whisper-base">whisper-base (balanced)</option>
                  </select>
                </Field>

                <Field label="Post-meeting accuracy" help="Re-transcribes with Parakeet-TDT. Adds ~1 GB sidecar download.">
                  <Toggle
                    label="Enable Parakeet retranscription"
                    sub="Runs after each meeting in the background."
                    checked={settings.parakeet_enabled}
                    onChange={(v) => {
                      set("parakeet_enabled", String(v));
                      persist("parakeet_enabled", String(v));
                    }}
                  />
                </Field>

                <Field label="Auto-start" help="Detects meeting audio and starts recording for you.">
                  <Toggle
                    label="Start when audio is detected"
                    sub="Asks before starting unless you opt in to silent."
                    checked={settings.auto_start}
                    onChange={(v) => {
                      set("auto_start", String(v));
                      persist("auto_start", String(v));
                      if (v) {
                        ipc.autoStartEnable().catch(console.error);
                      } else {
                        ipc.autoStartDisable().catch(console.error);
                      }
                    }}
                  />
                </Field>
              </Section>
            )}

            {/* Nudges & AI */}
            {activeSection === "nudges" && (
              <Section title="Nudges & AI" blurb="Quietly surfaces relevant notes from your knowledge base while you talk.">
                <Field label="Behaviour" help="Toggle the loop and any AI-generated talking points.">
                  <div className="flex flex-col">
                    <Toggle
                      label="KB nudges"
                      sub="Match recent transcript against your notes."
                      checked={settings.nudge_enabled}
                      onChange={(v) => {
                        set("nudge_enabled", String(v));
                        persist("nudge_enabled", String(v));
                      }}
                    />
                    <Toggle
                      label="AI talking points"
                      sub="Stream a short suggestion from llama-3.1-8b-instant."
                      checked={settings.ai_suggestions_enabled}
                      onChange={(v) => {
                        set("ai_suggestions_enabled", String(v));
                        persist("ai_suggestions_enabled", String(v));
                      }}
                    />
                  </div>
                </Field>

                <Field label="Search interval" help="How often the rolling window is rechecked.">
                  <div className="flex flex-col gap-1.5">
                    <input
                      type="range"
                      min={1}
                      max={15}
                      value={settings.nudge_interval_secs}
                      className="w-full accent-[var(--ink)]"
                      onChange={(e) => {
                        set("nudge_interval_secs", e.target.value);
                        persist("nudge_interval_secs", e.target.value);
                      }}
                    />
                    <div className="text-xs" style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}>
                      {settings.nudge_interval_secs}s · 1–15
                    </div>
                  </div>
                </Field>

                <Field label="Similarity threshold" help="Higher = stricter matches, fewer cards.">
                  <div className="flex flex-col gap-1.5">
                    <input
                      type="range"
                      min={0}
                      max={100}
                      value={Math.round(settings.nudge_threshold * 100)}
                      className="w-full accent-[var(--ink)]"
                      onChange={(e) => {
                        const v = (Number(e.target.value) / 100).toFixed(2);
                        set("nudge_threshold", v);
                        persist("nudge_threshold", v);
                      }}
                    />
                    <div className="text-xs" style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}>
                      {settings.nudge_threshold.toFixed(2)} cosine
                    </div>
                  </div>
                </Field>
              </Section>
            )}

            {/* Privacy */}
            {activeSection === "privacy" && (
              <Section title="Privacy" blurb="Everything stays on this machine unless you explicitly export it.">
                <Field
                  label="Screen-share invisibility"
                  help="Hides MeetAI windows from Zoom, Meet, Teams. macOS & Windows only — Linux can't enforce this."
                >
                  <Toggle
                    label="Hide from screen capture"
                    sub="Applies to main + overlay windows."
                    checked={settings.screen_share_protection}
                    onChange={(v) => {
                      set("screen_share_protection", String(v));
                      persist("screen_share_protection", String(v));
                    }}
                  />
                </Field>
              </Section>
            )}

            {/* About */}
            {activeSection === "about" && (
              <AboutSection />
            )}

            {/* Integrations */}
            {activeSection === "integrations" && (
              <Section title="Integrations" blurb="Optional bridges to other tools.">
                <Field label="Obsidian vault" help="Auto-export meeting summaries as markdown when a meeting ends.">
                  <div className="flex gap-2">
                    <input
                      className="input flex-1"
                      value={settings.obsidian_vault}
                      placeholder="~/Documents/MyVault"
                      style={{ fontFamily: "var(--font-mono)" }}
                      onChange={(e) => {
                        set("obsidian_vault", e.target.value);
                        persist("obsidian_vault", e.target.value);
                      }}
                    />
                    <button
                      className="btn"
                      onClick={async () => {
                        const path = await openDialog({ directory: true, multiple: false });
                        if (typeof path === "string") {
                          set("obsidian_vault", path);
                          persist("obsidian_vault", path);
                        }
                      }}
                    >
                      Choose…
                    </button>
                  </div>
                </Field>

                <Field label="Webhook" help="POST meeting JSON to this URL when a meeting ends. Leave blank to disable.">
                  <input
                    className="input w-full"
                    value={settings.webhook_url}
                    placeholder="https://…"
                    style={{ fontFamily: "var(--font-mono)" }}
                    onChange={(e) => {
                      set("webhook_url", e.target.value);
                      persist("webhook_url", e.target.value);
                    }}
                  />
                </Field>

                <Field label="MCP server" help="Exposes meetings to Claude Desktop / Cursor via a local stdio server.">
                  <div className="flex flex-col gap-4">
                    <Toggle
                      label="Enable MCP server"
                      sub="Lets external AI tools query your meeting history."
                      checked={settings.mcp_enabled}
                      onChange={(v) => {
                        set("mcp_enabled", String(v));
                        persist("mcp_enabled", String(v));
                      }}
                    />
                    {settings.mcp_enabled && <McpSnippetBlock />}
                  </div>
                </Field>
              </Section>
            )}
          </main>
        </div>
      </div>
    </div>
  );
}

// ── Sub-components ────────────────────────────────────────────────────────────

function Section({
  title,
  blurb,
  children,
}: {
  title: string;
  blurb: string;
  children: React.ReactNode;
}) {
  return (
    <section className="mb-10">
      <h3 className="text-base font-semibold m-0 mb-1 text-[var(--ink)]">{title}</h3>
      <p className="text-sm mt-0 mb-5" style={{ color: "var(--muted)" }}>
        {blurb}
      </p>
      <div className="flex flex-col gap-6">{children}</div>
    </section>
  );
}

function Field({
  label,
  help,
  children,
}: {
  label: string;
  help: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex gap-6">
      <div className="w-48 shrink-0">
        <div className="text-sm font-medium text-[var(--ink)]">{label}</div>
        <div className="text-xs mt-0.5" style={{ color: "var(--muted)" }}>
          {help}
        </div>
      </div>
      <div className="flex-1">{children}</div>
    </div>
  );
}

function Toggle({
  label,
  sub,
  checked,
  onChange,
}: {
  label: string;
  sub: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div
      className="flex items-center justify-between py-3 border-b border-[var(--border)] last:border-b-0"
      onClick={() => onChange(!checked)}
      style={{ cursor: "pointer" }}
    >
      <div>
        <div className="text-sm text-[var(--ink)]">{label}</div>
        <div className="text-xs mt-0.5" style={{ color: "var(--muted)" }}>
          {sub}
        </div>
      </div>
      <div
        className={`relative w-9 h-5 rounded-full transition-colors shrink-0 ${
          checked ? "bg-[var(--ink)]" : "bg-[var(--border)]"
        }`}
      >
        <div
          className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
            checked ? "translate-x-4" : "translate-x-0.5"
          }`}
        />
      </div>
    </div>
  );
}

function McpSnippetBlock() {
  const [snippet, setSnippet] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    ipc.mcpSnippet().then(setSnippet).catch(() => null);
  }, []);

  const handleCopy = async () => {
    if (!snippet) return;
    await navigator.clipboard.writeText(snippet).catch(console.error);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="text-xs leading-relaxed" style={{ color: "var(--muted)" }}>
        First build the MCP binary, then paste this config into{" "}
        <span style={{ fontFamily: "var(--font-mono)" }}>
          ~/.config/Claude/claude_desktop_config.json
        </span>
        :
      </div>
      <div
        className="text-xs p-3 rounded border border-[var(--border)] whitespace-pre overflow-x-auto"
        style={{
          background: "var(--surface-subtle)",
          fontFamily: "var(--font-mono)",
          color: "var(--ink)",
          lineHeight: 1.6,
        }}
      >
        {snippet ?? "Loading…"}
      </div>
      <div className="flex items-center gap-3">
        <button className="btn btn-sm btn-outline" onClick={handleCopy} disabled={!snippet}>
          {copied ? "Copied!" : "Copy config"}
        </button>
        <span className="text-xs" style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}>
          Build binary:{" "}
          <span style={{ color: "var(--ink)" }}>
            cargo build --release --bin meetai-mcp
          </span>
        </span>
      </div>
    </div>
  );
}

function AboutSection() {
  const [logPath, setLogPath] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState("");

  useEffect(() => {
    ipc.logFilePath().then(setLogPath).catch(() => null);
  }, []);

  const handleOpenLog = async () => {
    if (!logPath) return;
    setOpening(true);
    setOpenError("");
    try {
      await openPath(logPath);
    } catch {
      setOpenError("Could not open file — it may not exist yet (no warnings logged).");
    } finally {
      setOpening(false);
    }
  };

  const rows: [string, string][] = [
    ["Version", "0.1.0"],
    ["LLM provider", "Groq (llama-3.1-8b-instant · llama-3.3-70b-versatile)"],
    ["Embeddings", "fastembed bge-small-en-v1.5 · local CPU"],
    ["Transcription", "whisper-rs 0.16 · local CPU"],
    ["Platform", "Tauri v2 · React 19"],
  ];

  return (
    <Section title="About MeetAI" blurb="Version, data location, and diagnostic logs.">
      <Field label="Build info" help="Hard-coded at compile time.">
        <dl className="flex flex-col gap-1.5">
          {rows.map(([k, v]) => (
            <div key={k} className="flex gap-3 text-sm">
              <dt className="w-32 shrink-0 text-[var(--muted)]">{k}</dt>
              <dd style={{ color: "var(--ink)", fontFamily: k === "Version" ? "var(--font-mono)" : undefined }}>
                {v}
              </dd>
            </div>
          ))}
        </dl>
      </Field>

      <Field label="Log file" help="WARN-level events and panics are written here on each run.">
        <div className="flex flex-col gap-2">
          <div
            className="text-xs break-all"
            style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
          >
            {logPath ?? "Loading…"}
          </div>
          <div className="flex items-center gap-3">
            <button
              className="btn btn-sm btn-outline"
              onClick={handleOpenLog}
              disabled={!logPath || opening}
            >
              {opening ? "Opening…" : "Open log file"}
            </button>
            {openError && (
              <span className="text-xs" style={{ color: "var(--muted)" }}>
                {openError}
              </span>
            )}
          </div>
        </div>
      </Field>

      <Field label="Cold start" help="What runs at launch vs. on first use.">
        <dl className="flex flex-col gap-1.5 text-sm">
          {[
            ["At launch", "DB open + migrations, recover interrupted meetings"],
            ["On first KB op", "fastembed model downloads (~30 MB)"],
            ["On first meeting", "Whisper model loads from disk (~75 MB)"],
            ["Never at launch", "Whisper load, embed model init, Groq calls"],
          ].map(([k, v]) => (
            <div key={k} className="flex gap-3">
              <dt className="w-36 shrink-0" style={{ color: "var(--muted)" }}>{k}</dt>
              <dd style={{ color: "var(--ink)" }}>{v}</dd>
            </div>
          ))}
        </dl>
      </Field>

      <Field label="Memory" help="Measured over a 30-min meeting with all features active.">
        <div className="flex flex-col gap-1.5 text-sm" style={{ color: "var(--ink)" }}>
          <div>Audio: ring-buffered, 50 ms chunks, frames dropped when channel full.</div>
          <div>Transcript: written to SQLite (disk). Nudge cards: max 3 in memory.</div>
          <div>WAV: streamed to disk via hound — no in-memory accumulation.</div>
          <div style={{ color: "var(--muted)" }}>
            Expected RSS during meeting: &lt;250 MB (Whisper ~130 MB, embed ~60 MB, rest &lt;60 MB).
          </div>
        </div>
      </Field>
    </Section>
  );
}

function StatusBadge({ ok, children }: { ok: boolean; children: React.ReactNode }) {
  return (
    <div
      className="flex items-center gap-1.5 text-xs"
      style={{
        color: ok ? "var(--green)" : "var(--red)",
        fontFamily: "var(--font-mono)",
      }}
    >
      <span
        className={`inline-block w-1.5 h-1.5 rounded-full ${ok ? "bg-[var(--green)]" : "bg-[var(--red)]"}`}
      />
      {children}
    </div>
  );
}
