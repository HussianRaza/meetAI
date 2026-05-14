import { useEffect, useRef, useState, useCallback } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "../ipc";
import type { MeetingDetail } from "../ipc";
import Sidebar from "../components/Sidebar";

// ── Helpers ───────────────────────────────────────────────────────────────────

function fmtDuration(ms: number | null) {
  if (!ms) return "—";
  const s = Math.floor(ms / 1000);
  const m = Math.floor(s / 60);
  const h = Math.floor(m / 60);
  return h > 0 ? `${h}h ${m % 60}m` : `${m}m ${s % 60}s`;
}

function fmtTs(ms: number) {
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

// ── Job progress banner ───────────────────────────────────────────────────────

function JobBadge({
  kind,
  status,
}: {
  kind: string;
  status: string;
}) {
  const colors: Record<string, string> = {
    pending: "var(--muted)",
    running: "var(--amber)",
    done: "var(--green)",
    error: "var(--red)",
  };
  const icons: Record<string, string> = {
    pending: "·",
    running: "⟳",
    done: "✓",
    error: "✗",
  };
  return (
    <span
      className="inline-flex items-center gap-1 text-[10px] px-2 py-0.5 rounded border"
      style={{
        color: colors[status] ?? "var(--muted)",
        borderColor: colors[status] ?? "var(--border)",
        fontFamily: "var(--font-mono)",
      }}
    >
      <span>{icons[status] ?? "·"}</span>
      {kind}
    </span>
  );
}

// ── Tabs ──────────────────────────────────────────────────────────────────────

type Tab = "summary" | "transcript" | "notes" | "export";

function TabBar({
  active,
  onChange,
}: {
  active: Tab;
  onChange: (t: Tab) => void;
}) {
  const tabs: { key: Tab; label: string }[] = [
    { key: "summary", label: "Summary" },
    { key: "transcript", label: "Transcript" },
    { key: "notes", label: "Notes" },
    { key: "export", label: "Export" },
  ];
  return (
    <div className="flex border-b border-[var(--border)]">
      {tabs.map(({ key, label }) => (
        <button
          key={key}
          onClick={() => onChange(key)}
          className="px-5 py-2.5 text-sm border-b-2 transition-colors"
          style={{
            borderColor: active === key ? "var(--ink)" : "transparent",
            color: active === key ? "var(--ink)" : "var(--muted)",
            fontWeight: active === key ? 500 : 400,
          }}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

// ── Summary tab ───────────────────────────────────────────────────────────────

function SummaryTab({
  detail,
  onToggle,
  onRegenerate,
}: {
  detail: MeetingDetail;
  onToggle: (id: number, done: boolean) => void;
  onRegenerate: () => void;
}) {
  const processing = detail.status === "processing";
  const embedJob = detail.jobs.find((j) => j.kind === "Embed");
  const summaryJob = detail.jobs.find((j) => j.kind === "Summarize");

  return (
    <div className="flex flex-col gap-6 max-w-2xl">
      {/* Job progress */}
      {(processing || detail.jobs.length > 0) && (
        <div className="flex items-center gap-2">
          {embedJob && <JobBadge kind="Embed" status={embedJob.status} />}
          {summaryJob && <JobBadge kind="Summarize" status={summaryJob.status} />}
          {processing && (
            <span
              className="text-xs animate-pulse"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              Processing…
            </span>
          )}
        </div>
      )}

      {!detail.summary ? (
        <div
          className="text-sm"
          style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
        >
          {processing
            ? "Generating summary…"
            : "No summary yet. Click Regenerate to create one."}
        </div>
      ) : (
        <>
          {/* Overview */}
          <div>
            <div
              className="text-xs font-semibold uppercase tracking-widest mb-2"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              Overview
            </div>
            <p className="text-sm leading-relaxed" style={{ color: "var(--ink)" }}>
              {detail.summary.overview}
            </p>
          </div>

          {/* Decisions */}
          {detail.summary.decisions.length > 0 && (
            <div>
              <div
                className="text-xs font-semibold uppercase tracking-widest mb-2"
                style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
              >
                Key Decisions
              </div>
              <ul className="flex flex-col gap-1">
                {detail.summary.decisions.map((d, i) => (
                  <li key={i} className="text-sm flex gap-2" style={{ color: "var(--ink)" }}>
                    <span style={{ color: "var(--muted)" }}>—</span>
                    {d}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* Action items */}
          {detail.action_items.length > 0 && (
            <div>
              <div
                className="text-xs font-semibold uppercase tracking-widest mb-2"
                style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
              >
                Action Items
              </div>
              <ul className="flex flex-col gap-1.5">
                {detail.action_items.map((a) => (
                  <li key={a.id} className="flex items-start gap-2">
                    <input
                      type="checkbox"
                      checked={a.done}
                      onChange={(e) => onToggle(a.id, e.target.checked)}
                      className="mt-0.5 cursor-pointer"
                    />
                    <span
                      className="text-sm"
                      style={{
                        color: a.done ? "var(--muted)" : "var(--ink)",
                        textDecoration: a.done ? "line-through" : "none",
                      }}
                    >
                      {a.text}
                      {a.assignee && (
                        <span style={{ color: "var(--muted)" }}> → {a.assignee}</span>
                      )}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* Topics */}
          {detail.summary.topics.length > 0 && (
            <div>
              <div
                className="text-xs font-semibold uppercase tracking-widest mb-2"
                style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
              >
                Topics
              </div>
              <div className="flex flex-wrap gap-1.5">
                {detail.summary.topics.map((t, i) => (
                  <span
                    key={i}
                    className="text-xs px-2 py-0.5 rounded"
                    style={{
                      background: "var(--surface-subtle)",
                      color: "var(--ink)",
                      fontFamily: "var(--font-mono)",
                    }}
                  >
                    {t}
                  </span>
                ))}
              </div>
            </div>
          )}
        </>
      )}

      <div>
        <button className="btn btn-sm btn-outline" onClick={onRegenerate}>
          Regenerate Summary
        </button>
      </div>
    </div>
  );
}

// ── Transcript tab ────────────────────────────────────────────────────────────

function TranscriptTab({ detail }: { detail: MeetingDetail }) {
  return (
    <div className="flex flex-col gap-3 max-w-2xl">
      {detail.segments.length === 0 ? (
        <div
          className="text-sm"
          style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
        >
          No transcript segments.
        </div>
      ) : (
        detail.segments.map((seg, i) => {
          const speaker =
            seg.speaker_name ??
            (seg.source === "you" ? "You" : "Speaker");
          return (
            <div key={i} className="flex flex-col gap-0.5">
              <div className="flex items-baseline gap-2">
                <span
                  className="text-xs font-semibold"
                  style={{
                    color: seg.source === "you" ? "var(--blue)" : "var(--green)",
                    fontFamily: "var(--font-mono)",
                  }}
                >
                  {speaker}
                </span>
                <span
                  className="text-[10px]"
                  style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
                >
                  {fmtTs(seg.start_ms)}
                </span>
              </div>
              <div className="text-sm leading-relaxed" style={{ color: "var(--ink)" }}>
                {seg.text}
              </div>
            </div>
          );
        })
      )}
    </div>
  );
}

// ── Notes tab ─────────────────────────────────────────────────────────────────

function NotesTab({ detail }: { detail: MeetingDetail }) {
  const [notes, setNotes] = useState(detail.notes ?? "");
  const [saved, setSaved] = useState(true);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleChange = (v: string) => {
    setNotes(v);
    setSaved(false);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(async () => {
      await ipc.meetingNotesSave(detail.id, v).catch(console.error);
      setSaved(true);
    }, 600);
  };

  return (
    <div className="flex flex-col gap-2 max-w-2xl flex-1">
      <textarea
        className="input flex-1 resize-none text-sm min-h-[320px]"
        placeholder="Personal notes about this meeting…"
        value={notes}
        onChange={(e) => handleChange(e.target.value)}
        style={{ lineHeight: 1.7 }}
      />
      <div
        className="text-xs"
        style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
      >
        {saved ? "Saved" : "Saving…"}
      </div>
    </div>
  );
}

// ── Export tab ────────────────────────────────────────────────────────────────

function ExportTab({ detail }: { detail: MeetingDetail }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      const md = await ipc.meetingExportMarkdown(detail.id);
      await navigator.clipboard.writeText(md);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      console.error(e);
    }
  };

  const handleDownload = async () => {
    try {
      const md = await ipc.meetingExportMarkdown(detail.id);
      const blob = new Blob([md], { type: "text/markdown" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${detail.title.replace(/[^a-zA-Z0-9 ]/g, "")}.md`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      console.error(e);
    }
  };

  return (
    <div className="flex flex-col gap-4 max-w-sm">
      <div
        className="text-xs"
        style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
      >
        Export includes summary, action items, topics, and full transcript with timestamps.
      </div>
      <div className="flex flex-col gap-2">
        <button className="btn btn-primary" onClick={handleDownload}>
          Download Markdown (.md)
        </button>
        <button className="btn btn-outline" onClick={handleCopy}>
          {copied ? "Copied!" : "Copy to Clipboard"}
        </button>
      </div>
    </div>
  );
}

// ── Main screen ───────────────────────────────────────────────────────────────

export default function PostMeeting() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [tab, setTab] = useState<Tab>("summary");
  const [error, setError] = useState("");

  const load = useCallback(() => {
    if (!id) return;
    ipc.meetingGet(id).then(setDetail).catch((e) => setError(String(e)));
  }, [id]);

  useEffect(() => {
    load();
  }, [load]);

  // Refresh on job-progress events for this meeting
  useEffect(() => {
    if (!id) return;
    const unlisten = listen<{ meeting_id: string }>("job-progress", (e) => {
      if (e.payload.meeting_id === id) load();
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [id, load]);

  const handleToggle = async (itemId: number, done: boolean) => {
    await ipc.actionItemToggle(itemId, done).catch(console.error);
    setDetail((d) =>
      d
        ? {
            ...d,
            action_items: d.action_items.map((a) =>
              a.id === itemId ? { ...a, done } : a
            ),
          }
        : d
    );
  };

  const handleRegenerate = async () => {
    if (!id) return;
    await ipc.meetingRegenerateSummary(id).catch(console.error);
  };

  if (error) {
    return (
      <div className="flex h-screen">
        <Sidebar groqOk={null} />
        <div className="flex flex-col flex-1 items-center justify-center gap-4">
          <div className="text-sm" style={{ color: "var(--red)" }}>
            {error}
          </div>
          <button className="btn btn-outline" onClick={() => navigate("/library")}>
            Back to Library
          </button>
        </div>
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="flex h-screen">
        <Sidebar groqOk={null} />
        <div className="flex flex-1 items-center justify-center text-sm"
          style={{ color: "var(--muted)" }}>
          Loading…
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-screen">
      <Sidebar groqOk={null} />
      <div className="flex flex-col flex-1 min-w-0">
        {/* Header */}
        <header
          className="flex items-start justify-between px-6 py-4 border-b border-[var(--border)]"
          style={{ background: "var(--surface)" }}
        >
          <div className="flex flex-col gap-1 min-w-0">
            <div
              className="text-xl font-semibold truncate"
              style={{ fontFamily: "var(--font-serif)", color: "var(--ink)" }}
            >
              {detail.title}
            </div>
            <div
              className="text-xs flex items-center gap-2"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              <span>{new Date(detail.started_at).toLocaleDateString()}</span>
              <span>·</span>
              <span>{fmtDuration(detail.duration_ms)}</span>
              <span>·</span>
              <span>{detail.segments.length} segments</span>
            </div>
          </div>
          <button
            className="btn btn-sm btn-outline shrink-0"
            onClick={() => navigate("/library")}
          >
            ← Library
          </button>
        </header>

        <TabBar active={tab} onChange={setTab} />

        <div className="flex-1 overflow-y-auto px-6 py-5">
          {tab === "summary" && (
            <SummaryTab
              detail={detail}
              onToggle={handleToggle}
              onRegenerate={handleRegenerate}
            />
          )}
          {tab === "transcript" && <TranscriptTab detail={detail} />}
          {tab === "notes" && <NotesTab detail={detail} />}
          {tab === "export" && <ExportTab detail={detail} />}
        </div>
      </div>
    </div>
  );
}
