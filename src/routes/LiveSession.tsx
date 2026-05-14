import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "../ipc";
import type { WhisperStatus, TranscriptSegment } from "../ipc";
import { useSessionStore } from "../stores/session";
import Sidebar from "../components/Sidebar";

// ── Setup panel (before meeting starts) ─────────────────────────────────────

function ModelBanner({
  status,
  onDownload,
  downloading,
  progress,
}: {
  status: WhisperStatus | null;
  onDownload: () => void;
  downloading: boolean;
  progress: number;
}) {
  if (!status) return null;
  if (status.ready)
    return (
      <div
        className="badge badge-success text-xs"
        style={{ fontFamily: "var(--font-mono)" }}
      >
        {status.model_name} · ready
      </div>
    );
  if (downloading)
    return (
      <div className="flex items-center gap-3">
        <div
          className="text-xs"
          style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
        >
          Downloading {status.model_name}… {progress}%
        </div>
        <div className="h-1.5 w-32 rounded-full bg-[var(--surface-subtle)] overflow-hidden">
          <div
            className="h-full bg-[var(--amber)] transition-all"
            style={{ width: `${progress}%` }}
          />
        </div>
      </div>
    );
  return (
    <div className="flex items-center gap-3">
      <span className="text-xs" style={{ color: "var(--red)", fontFamily: "var(--font-mono)" }}>
        {status.model_name} not downloaded
      </span>
      <button className="btn btn-sm btn-outline" onClick={onDownload}>
        Download (~75 MB)
      </button>
    </div>
  );
}

function SetupPanel() {
  const navigate = useNavigate();
  const { title, setTitle, startRecording } = useSessionStore();
  const [status, setStatus] = useState<WhisperStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    ipc.whisperModelStatus().then(setStatus).catch(() => null);

    const unlisten = listen<{
      percent: number;
      done: boolean;
    }>("whisper-download-progress", (e) => {
      setProgress(e.payload.percent);
      if (e.payload.done) {
        setDownloading(false);
        ipc.whisperModelStatus().then(setStatus).catch(() => null);
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const handleDownload = async () => {
    if (!status) return;
    setDownloading(true);
    setProgress(0);
    ipc.whisperDownloadModel(status.model_name).catch((e) => {
      setError(String(e));
      setDownloading(false);
    });
  };

  const handleStart = async () => {
    if (!status?.ready || starting) return;
    setError("");
    setStarting(true);
    try {
      const meetingTitle = title.trim() || "Untitled Meeting";
      const id = await ipc.meetingStart(meetingTitle);
      startRecording(id, meetingTitle);
      navigate("/live");
    } catch (e) {
      setError(String(e));
    } finally {
      setStarting(false);
    }
  };

  return (
    <div className="flex h-screen">
      <Sidebar groqOk={null} />
      <main className="flex flex-col flex-1 items-center justify-center gap-8 p-10">
        <div className="w-full max-w-md flex flex-col gap-6">
          <div>
            <div
              className="text-2xl font-semibold"
              style={{ fontFamily: "var(--font-serif)", color: "var(--ink)" }}
            >
              New Meeting
            </div>
            <div
              className="mt-1 text-sm"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              Mic + system audio → live transcript
            </div>
          </div>

          <input
            className="input w-full text-lg"
            placeholder="Meeting title (optional)"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleStart()}
          />

          <div className="flex flex-col gap-2">
            <ModelBanner
              status={status}
              onDownload={handleDownload}
              downloading={downloading}
              progress={progress}
            />
          </div>

          {error && (
            <div
              className="text-xs px-3 py-2 rounded"
              style={{
                background: "var(--surface-subtle)",
                color: "var(--red)",
                fontFamily: "var(--font-mono)",
              }}
            >
              {error}
            </div>
          )}

          <button
            className="btn btn-primary"
            disabled={!status?.ready || starting || downloading}
            onClick={handleStart}
          >
            {starting ? "Starting…" : "Start Meeting"}
          </button>

          <div
            className="text-xs"
            style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
          >
            Note: System audio capture requires a PulseAudio/PipeWire monitor
            source. Mic-only works on all setups.
          </div>
        </div>
      </main>
    </div>
  );
}

// ── Live recording panel ─────────────────────────────────────────────────────

function useTimer(startedAt: number | null) {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    if (!startedAt) return;
    const id = setInterval(() => setElapsed(Date.now() - startedAt), 500);
    return () => clearInterval(id);
  }, [startedAt]);
  const s = Math.floor(elapsed / 1000);
  const m = Math.floor(s / 60);
  const h = Math.floor(m / 60);
  if (h > 0) return `${h}:${String(m % 60).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
  return `${m}:${String(s % 60).padStart(2, "0")}`;
}

function RecordingPanel() {
  const navigate = useNavigate();
  const { title, startedAt, transcript, stopRecording } =
    useSessionStore();
  const timer = useTimer(startedAt);
  const bottomRef = useRef<HTMLDivElement>(null);
  const [stopping, setStopping] = useState(false);

  // Subscribe to transcript-segment events
  useEffect(() => {
    const { addSegment } = useSessionStore.getState();
    const unlisten = listen<TranscriptSegment>("transcript-segment", (e) => {
      addSegment(e.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  // Auto-scroll to bottom
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [transcript]);

  const handleStop = async () => {
    setStopping(true);
    try {
      await ipc.meetingStop();
      stopRecording();
      navigate("/settings");
    } catch (e) {
      console.error("stop error", e);
      setStopping(false);
    }
  };

  return (
    <div className="flex h-screen">
      <Sidebar groqOk={null} />
      <div className="flex flex-col flex-1 min-w-0">
        {/* Top bar */}
        <header
          className="flex items-center justify-between px-6 py-3 border-b border-[var(--border)]"
          style={{ background: "var(--surface)" }}
        >
          <div className="flex items-center gap-3">
            <span
              className="inline-block w-2 h-2 rounded-full animate-pulse"
              style={{ background: "var(--red)" }}
            />
            <span
              className="font-medium"
              style={{ color: "var(--ink)" }}
            >
              {title || "Untitled Meeting"}
            </span>
            <span
              className="text-sm"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              {timer}
            </span>
          </div>
          <button
            className="btn btn-sm btn-outline"
            onClick={handleStop}
            disabled={stopping}
          >
            {stopping ? "Stopping…" : "Stop"}
          </button>
        </header>

        {/* Body: transcript + nudge placeholder */}
        <div className="flex flex-1 min-h-0">
          {/* Transcript panel */}
          <div className="flex flex-col flex-1 min-w-0 border-r border-[var(--border)]">
            <div
              className="px-4 py-2 text-xs font-semibold tracking-widest uppercase border-b border-[var(--border)]"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              Transcript
            </div>
            <div className="flex-1 overflow-y-auto px-6 py-4 flex flex-col gap-3">
              {transcript.length === 0 && (
                <div
                  className="text-sm mt-6"
                  style={{
                    color: "var(--muted)",
                    fontFamily: "var(--font-mono)",
                  }}
                >
                  Listening… speak into your mic or play audio on your speakers.
                </div>
              )}
              {transcript.map((seg) => (
                <div key={seg.id} className="flex flex-col gap-0.5">
                  <div
                    className="text-xs font-semibold"
                    style={{
                      color:
                        seg.source === "you"
                          ? "var(--blue)"
                          : "var(--green)",
                      fontFamily: "var(--font-mono)",
                    }}
                  >
                    {seg.source === "you" ? "You" : "Speaker"}
                  </div>
                  <div
                    className="text-sm"
                    style={{ color: "var(--ink)", lineHeight: 1.6 }}
                  >
                    {seg.text}
                  </div>
                </div>
              ))}
              <div ref={bottomRef} />
            </div>
          </div>

          {/* Nudge panel placeholder */}
          <div
            className="w-72 shrink-0 flex flex-col"
            style={{ background: "var(--surface)" }}
          >
            <div
              className="px-4 py-2 text-xs font-semibold tracking-widest uppercase border-b border-[var(--border)]"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              AI Nudges
            </div>
            <div
              className="flex-1 flex items-center justify-center text-sm px-6 text-center"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              KB nudges coming in M4
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Route entry ──────────────────────────────────────────────────────────────

export default function LiveSession() {
  const { isRecording } = useSessionStore();
  return isRecording ? <RecordingPanel /> : <SetupPanel />;
}
