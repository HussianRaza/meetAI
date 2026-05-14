import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

interface TranscriptSegment {
  meeting_id: string;
  source: "you" | "speaker";
  text: string;
  start_ms: number;
  is_final: boolean;
}

interface NudgeCard {
  id: string;
  file_path: string;
  breadcrumb: string;
  snippet: string;
  score: number;
  suggestion?: string;
}

const STYLE = {
  root: {
    width: "100%",
    height: "100%",
    background: "rgba(13, 13, 15, 0.88)",
    backdropFilter: "blur(12px)",
    WebkitBackdropFilter: "blur(12px)",
    borderRadius: "12px",
    padding: "12px",
    display: "flex",
    flexDirection: "column" as const,
    gap: "8px",
    fontFamily: "system-ui, -apple-system, sans-serif",
    color: "#f5f5f5",
    fontSize: "12px",
    userSelect: "none" as const,
  },
  transcriptArea: {
    flex: 1,
    display: "flex",
    flexDirection: "column" as const,
    justifyContent: "flex-end",
    gap: "4px",
    overflow: "hidden",
  },
  nudge: {
    background: "rgba(232, 164, 69, 0.15)",
    borderLeft: "2px solid #e8a445",
    padding: "6px 8px",
    borderRadius: "4px",
  },
  nudgeBreadcrumb: {
    color: "#e8a445",
    fontSize: "10px",
    fontFamily: "monospace",
    marginBottom: "2px",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap" as const,
  },
  nudgeText: {
    color: "rgba(255,255,255,0.85)",
    lineHeight: 1.4,
    overflow: "hidden",
    display: "-webkit-box" as const,
    WebkitLineClamp: 2,
    WebkitBoxOrient: "vertical" as const,
  },
  controls: {
    display: "flex",
    gap: "6px",
    flexShrink: 0,
  },
  btnBase: {
    flex: 1,
    border: "1px solid rgba(255,255,255,0.2)",
    borderRadius: "6px",
    color: "white",
    fontSize: "11px",
    padding: "5px 8px",
    cursor: "pointer",
    background: "rgba(255,255,255,0.08)",
    transition: "background 0.15s",
  },
  btnStop: {
    background: "rgba(200, 60, 60, 0.25)",
    border: "1px solid rgba(200, 60, 60, 0.45)",
  },
} as const;

export default function Overlay() {
  const [lines, setLines] = useState<{ text: string; source: "you" | "speaker" }[]>([]);
  const [nudge, setNudge] = useState<NudgeCard | null>(null);

  useEffect(() => {
    const unSeg = listen<TranscriptSegment>("transcript-segment", (e) => {
      if (e.payload.is_final) {
        setLines((prev) => {
          const entry = { text: e.payload.text, source: e.payload.source };
          return [...prev, entry].slice(-3);
        });
      }
    });

    const unNudge = listen<NudgeCard>("nudge-update", (e) => {
      setNudge(e.payload);
    });

    return () => {
      unSeg.then((f) => f());
      unNudge.then((f) => f());
    };
  }, []);

  const handleExpand = async () => {
    await getCurrentWindow().hide();
    const main = await WebviewWindow.getByLabel("main");
    if (main) {
      await main.show();
      await main.setFocus();
    }
  };

  const handleStop = async () => {
    await invoke("meeting_stop").catch(console.error);
    setLines([]);
    setNudge(null);
    await getCurrentWindow().hide();
  };

  return (
    <div style={STYLE.root}>
      <div style={STYLE.transcriptArea}>
        {lines.length === 0 ? (
          <div style={{ color: "rgba(255,255,255,0.35)", fontFamily: "monospace" }}>
            Listening…
          </div>
        ) : (
          lines.map((line, i) => {
            const opacity = 0.45 + 0.55 * ((i + 1) / lines.length);
            const color = line.source === "you" ? "#7aabf5" : "#6ecfa2";
            return (
              <div
                key={i}
                style={{ opacity, lineHeight: 1.45, fontSize: "11.5px" }}
              >
                <span style={{ color, fontFamily: "monospace", fontSize: "10px", marginRight: "4px" }}>
                  {line.source === "you" ? "You" : "Speaker"}
                </span>
                {line.text}
              </div>
            );
          })
        )}
      </div>

      {nudge && (
        <div style={STYLE.nudge}>
          <div style={STYLE.nudgeBreadcrumb}>{nudge.breadcrumb}</div>
          <div style={STYLE.nudgeText}>
            {nudge.suggestion ?? nudge.snippet}
          </div>
        </div>
      )}

      <div style={STYLE.controls}>
        <button style={STYLE.btnBase} onClick={handleExpand}>
          Expand ↗
        </button>
        <button style={{ ...STYLE.btnBase, ...STYLE.btnStop }} onClick={handleStop}>
          Stop
        </button>
      </div>
    </div>
  );
}
