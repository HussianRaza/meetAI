import { useEffect, useState } from "react";
import { MemoryRouter, Routes, Route, Navigate, useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import Settings from "./routes/Settings";
import LiveSession from "./routes/LiveSession";
import Library from "./routes/Library";
import Chat from "./routes/Chat";
import PostMeeting from "./routes/PostMeeting";
import Onboarding from "./routes/Onboarding";
import { ipc } from "./ipc";
import { useSessionStore } from "./stores/session";

// ── Meeting-detected notification banner ──────────────────────────────────────

function DetectedBanner({ onDismiss }: { onDismiss: () => void }) {
  const navigate = useNavigate();
  const [title, setTitle] = useState("");

  const handleStart = async () => {
    navigate("/live");
    onDismiss();
  };

  return (
    <div
      style={{
        position: "fixed",
        top: 12,
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 9999,
        display: "flex",
        alignItems: "center",
        gap: 10,
        background: "var(--ink)",
        color: "var(--paper)",
        borderRadius: 8,
        padding: "10px 14px",
        boxShadow: "0 4px 20px rgba(0,0,0,0.3)",
        fontSize: 13,
        whiteSpace: "nowrap",
        maxWidth: "calc(100vw - 48px)",
      }}
    >
      <span
        style={{
          display: "inline-block",
          width: 7,
          height: 7,
          borderRadius: "50%",
          background: "var(--red)",
          flexShrink: 0,
        }}
      />
      <span style={{ color: "rgba(255,255,255,0.7)" }}>Meeting audio detected —</span>
      <input
        placeholder="Meeting title…"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && handleStart()}
        style={{
          background: "rgba(255,255,255,0.1)",
          border: "1px solid rgba(255,255,255,0.2)",
          borderRadius: 4,
          color: "white",
          padding: "3px 8px",
          fontSize: 12,
          fontFamily: "var(--font-mono)",
          width: 160,
          outline: "none",
        }}
        autoFocus
      />
      <button
        onClick={handleStart}
        style={{
          background: "var(--green)",
          color: "white",
          border: "none",
          borderRadius: 4,
          padding: "4px 10px",
          fontSize: 12,
          cursor: "pointer",
          flexShrink: 0,
        }}
      >
        Start
      </button>
      <button
        onClick={onDismiss}
        style={{
          background: "none",
          color: "rgba(255,255,255,0.5)",
          border: "none",
          fontSize: 16,
          cursor: "pointer",
          lineHeight: 1,
          padding: "0 4px",
          flexShrink: 0,
        }}
      >
        ×
      </button>
    </div>
  );
}

// ── Inner app (inside MemoryRouter so useNavigate works) ──────────────────────

function AppRoutes() {
  const { isRecording } = useSessionStore();
  const [detectedBanner, setDetectedBanner] = useState(false);

  useEffect(() => {
    const unlisten = listen<void>("meeting-detected", () => {
      if (!useSessionStore.getState().isRecording) {
        setDetectedBanner(true);
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  return (
    <>
      {detectedBanner && !isRecording && (
        <DetectedBanner onDismiss={() => setDetectedBanner(false)} />
      )}
      <Routes>
        <Route path="/onboarding" element={<Onboarding />} />
        <Route path="/settings" element={<Settings />} />
        <Route path="/live" element={<LiveSession />} />
        <Route path="/library" element={<Library />} />
        <Route path="/chat" element={<Chat />} />
        <Route path="/meeting/:id" element={<PostMeeting />} />
        <Route path="*" element={<Navigate to="/library" replace />} />
      </Routes>
    </>
  );
}

// ── Root component ────────────────────────────────────────────────────────────

export default function App() {
  const [initialRoute, setInitialRoute] = useState<string | null>(null);

  useEffect(() => {
    ipc
      .settingsGet()
      .then((s) =>
        setInitialRoute(s.groq_key.trim() ? "/library" : "/onboarding")
      )
      .catch(() => setInitialRoute("/library"));
  }, []);

  if (!initialRoute) {
    return (
      <div
        style={{
          display: "flex",
          height: "100vh",
          alignItems: "center",
          justifyContent: "center",
          background: "var(--paper)",
          color: "var(--muted)",
          fontFamily: "var(--font-mono)",
          fontSize: 12,
        }}
      >
        Loading…
      </div>
    );
  }

  return (
    <MemoryRouter initialEntries={[initialRoute]}>
      <AppRoutes />
    </MemoryRouter>
  );
}
