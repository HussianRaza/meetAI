import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ipc } from "../ipc";
import type { MeetingRow } from "../ipc";
import Sidebar from "../components/Sidebar";
import { useSessionStore } from "../stores/session";

// ── Helpers ───────────────────────────────────────────────────────────────────

function fmtDuration(ms: number | null): string {
  if (!ms) return "—";
  const s = Math.floor(ms / 1000);
  const m = Math.floor(s / 60);
  const h = Math.floor(m / 60);
  if (h > 0) return `${h}h ${m % 60}m`;
  if (m > 0) return `${m}m ${s % 60}s`;
  return `${s}s`;
}

function fmtDate(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, {
    weekday: "long",
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

function fmtTime(ms: number): string {
  return new Date(ms).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function dayKey(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

function groupByDay(meetings: MeetingRow[]): [string, MeetingRow[]][] {
  const map = new Map<string, MeetingRow[]>();
  for (const m of meetings) {
    const k = dayKey(m.started_at);
    if (!map.has(k)) map.set(k, []);
    map.get(k)!.push(m);
  }
  return Array.from(map.entries());
}

function StatusBadge({ status }: { status: MeetingRow["status"] }) {
  const colors: Record<MeetingRow["status"], string> = {
    recording: "var(--red)",
    processing: "var(--amber)",
    done: "var(--green)",
    error: "var(--red)",
  };
  return (
    <span
      className="text-[10px] px-1.5 py-0.5 rounded border"
      style={{
        color: colors[status],
        borderColor: colors[status],
        fontFamily: "var(--font-mono)",
      }}
    >
      {status}
    </span>
  );
}

// ── Meeting row ───────────────────────────────────────────────────────────────

function MeetingItem({ meeting }: { meeting: MeetingRow }) {
  const navigate = useNavigate();

  const handleClick = () => {
    // M6 will implement the post-meeting view
    if (meeting.status === "recording") {
      navigate("/live");
    }
    // else: navigate to /meeting/:id (M6)
  };

  return (
    <button
      onClick={handleClick}
      className="flex items-center justify-between w-full px-4 py-3 rounded hover:bg-[var(--surface-subtle)] transition-colors text-left"
    >
      <div className="flex flex-col gap-0.5 min-w-0">
        <div
          className="font-medium truncate"
          style={{ color: "var(--ink)" }}
        >
          {meeting.title}
        </div>
        <div
          className="text-xs flex items-center gap-2"
          style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
        >
          <span>{fmtTime(meeting.started_at)}</span>
          <span>·</span>
          <span>{fmtDuration(meeting.duration_ms)}</span>
          {meeting.platform && (
            <>
              <span>·</span>
              <span>{meeting.platform}</span>
            </>
          )}
          <span>·</span>
          <span>{meeting.segment_count} segments</span>
        </div>
      </div>
      <StatusBadge status={meeting.status} />
    </button>
  );
}

// ── Main screen ───────────────────────────────────────────────────────────────

export default function Library() {
  const { isRecording } = useSessionStore();
  const [meetings, setMeetings] = useState<MeetingRow[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadMeetings = (q: string) => {
    setLoading(true);
    const req = q.trim()
      ? ipc.meetingSearch(q)
      : ipc.meetingsList();
    req
      .then(setMeetings)
      .catch(console.error)
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    loadMeetings("");
  }, []);

  const handleSearch = (q: string) => {
    setQuery(q);
    if (searchTimer.current) clearTimeout(searchTimer.current);
    searchTimer.current = setTimeout(() => loadMeetings(q), 300);
  };

  const groups = groupByDay(meetings);

  return (
    <div className="flex h-screen">
      <Sidebar groqOk={null} />
      <div className="flex flex-col flex-1 min-w-0">
        {/* Header */}
        <header
          className="flex items-center gap-4 px-6 py-4 border-b border-[var(--border)]"
          style={{ background: "var(--surface)" }}
        >
          <div
            className="text-lg font-semibold"
            style={{ fontFamily: "var(--font-serif)", color: "var(--ink)" }}
          >
            Library
          </div>
          <input
            className="input flex-1 max-w-sm text-sm"
            placeholder="Search meetings…"
            value={query}
            onChange={(e) => handleSearch(e.target.value)}
          />
          <div
            className="text-xs"
            style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
          >
            {meetings.length} meeting{meetings.length !== 1 ? "s" : ""}
          </div>
        </header>

        {/* Content */}
        <div className="flex-1 overflow-y-auto px-6 py-4">
          {loading ? (
            <div
              className="text-sm text-center mt-10"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              Loading…
            </div>
          ) : meetings.length === 0 ? (
            <div
              className="text-sm text-center mt-10 flex flex-col gap-2"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              <div>No meetings yet.</div>
              {!isRecording && (
                <div>Click "+ New Meeting" in the sidebar to start recording.</div>
              )}
            </div>
          ) : (
            <div className="flex flex-col gap-6">
              {groups.map(([key, dayMeetings]) => (
                <div key={key}>
                  <div
                    className="text-xs font-semibold uppercase tracking-widest mb-2 px-4"
                    style={{
                      color: "var(--muted)",
                      fontFamily: "var(--font-mono)",
                    }}
                  >
                    {fmtDate(dayMeetings[0].started_at)}
                  </div>
                  <div className="flex flex-col divide-y divide-[var(--border)] border border-[var(--border)] rounded">
                    {dayMeetings.map((m) => (
                      <MeetingItem key={m.id} meeting={m} />
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
