import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ipc } from "../ipc";

type Step = 1 | 2 | 3;

export default function Onboarding() {
  const navigate = useNavigate();
  const [step, setStep] = useState<Step>(1);
  const [groqKey, setGroqKey] = useState("");
  const [keyStatus, setKeyStatus] = useState<"idle" | "testing" | "ok" | "error">("idle");
  const [kbFolder, setKbFolder] = useState("");

  const testKey = async () => {
    if (!groqKey.trim()) return;
    setKeyStatus("testing");
    try {
      const ok = await ipc.groqTestConnection(groqKey.trim());
      setKeyStatus(ok ? "ok" : "error");
    } catch {
      setKeyStatus("error");
    }
  };

  const saveKeyAndNext = async () => {
    if (groqKey.trim()) {
      await ipc.settingsSet("groq_key", groqKey.trim()).catch(console.error);
    }
    setStep(3);
  };

  const chooseFolder = async () => {
    const path = await openDialog({ directory: true, multiple: false });
    if (typeof path === "string") {
      setKbFolder(path);
    }
  };

  const finish = async () => {
    if (kbFolder) {
      await ipc.settingsSet("kb_folder", kbFolder).catch(console.error);
      ipc.kbIndexStart(kbFolder).catch(console.error); // fire and forget
    }
    navigate("/library");
  };

  return (
    <div
      className="flex h-screen items-center justify-center"
      style={{ background: "var(--paper)" }}
    >
      <div className="flex flex-col items-center w-full max-w-md px-8">
        {/* Logo / wordmark */}
        <div
          className="text-3xl font-normal mb-10"
          style={{ fontFamily: "var(--font-serif)", color: "var(--ink)" }}
        >
          MeetAI
        </div>

        {/* Step indicator */}
        <div className="flex items-center gap-2 mb-10">
          {([1, 2, 3] as Step[]).map((n) => (
            <div key={n} className="flex items-center gap-2">
              <div
                className="w-6 h-6 rounded-full flex items-center justify-center text-[11px]"
                style={{
                  background: step >= n ? "var(--ink)" : "var(--surface-subtle)",
                  color: step >= n ? "var(--paper)" : "var(--muted)",
                  fontFamily: "var(--font-mono)",
                  transition: "background 0.2s",
                }}
              >
                {n}
              </div>
              {n < 3 && (
                <div
                  className="w-8 h-px"
                  style={{ background: step > n ? "var(--ink)" : "var(--border)" }}
                />
              )}
            </div>
          ))}
        </div>

        {/* ── Step 1: Welcome ── */}
        {step === 1 && (
          <div className="flex flex-col gap-6 w-full">
            <div>
              <h2
                className="text-xl font-normal mb-2"
                style={{ fontFamily: "var(--font-serif)", color: "var(--ink)" }}
              >
                Welcome
              </h2>
              <p className="text-sm leading-relaxed" style={{ color: "var(--muted)" }}>
                MeetAI transcribes your meetings in real time, surfaces relevant notes from your
                knowledge base, and generates structured summaries — all on this machine. Nothing
                leaves your device except AI inference calls to Groq.
              </p>
            </div>

            <div
              className="flex flex-col gap-3 p-4 rounded border border-[var(--border)]"
              style={{ background: "var(--surface)", fontSize: "13px", color: "var(--ink)" }}
            >
              {[
                ["Live transcription", "Whisper runs locally, two audio channels"],
                ["KB nudges", "Your notes surface automatically as you speak"],
                ["AI summaries", "Decisions, action items, topics — auto-extracted"],
              ].map(([label, sub]) => (
                <div key={label} className="flex flex-col gap-0.5">
                  <span className="font-medium">{label}</span>
                  <span className="text-xs" style={{ color: "var(--muted)" }}>{sub}</span>
                </div>
              ))}
            </div>

            <button className="btn btn-primary w-full" onClick={() => setStep(2)}>
              Get started
            </button>
          </div>
        )}

        {/* ── Step 2: Groq API key ── */}
        {step === 2 && (
          <div className="flex flex-col gap-5 w-full">
            <div>
              <h2
                className="text-xl font-normal mb-2"
                style={{ fontFamily: "var(--font-serif)", color: "var(--ink)" }}
              >
                Connect Groq
              </h2>
              <p className="text-sm leading-relaxed" style={{ color: "var(--muted)" }}>
                MeetAI uses Groq for live nudges and post-meeting summaries. The free tier is
                plenty for personal use.
              </p>
            </div>

            <div className="flex flex-col gap-2">
              <button
                className="text-left text-xs underline"
                style={{ color: "var(--blue)", fontFamily: "var(--font-mono)", background: "none", border: "none", cursor: "pointer", padding: 0 }}
                onClick={() => openUrl("https://console.groq.com/keys").catch(console.error)}
              >
                console.groq.com/keys — get a free key
              </button>
              <div className="flex gap-2">
                <input
                  type="password"
                  className="input flex-1"
                  placeholder="gsk_…"
                  value={groqKey}
                  onChange={(e) => {
                    setGroqKey(e.target.value);
                    setKeyStatus("idle");
                  }}
                  onKeyDown={(e) => e.key === "Enter" && testKey()}
                  style={{ fontFamily: "var(--font-mono)" }}
                  autoFocus
                />
                <button
                  className="btn"
                  onClick={testKey}
                  disabled={keyStatus === "testing" || !groqKey.trim()}
                >
                  {keyStatus === "testing" ? "Testing…" : "Test"}
                </button>
              </div>
              {keyStatus === "ok" && (
                <div className="text-xs flex items-center gap-1.5" style={{ color: "var(--green)", fontFamily: "var(--font-mono)" }}>
                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-[var(--green)]" />
                  Connected
                </div>
              )}
              {keyStatus === "error" && (
                <div className="text-xs flex items-center gap-1.5" style={{ color: "var(--red)", fontFamily: "var(--font-mono)" }}>
                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-[var(--red)]" />
                  Connection failed — check the key
                </div>
              )}
            </div>

            <div className="flex gap-2">
              <button className="btn btn-outline flex-1" onClick={() => setStep(1)}>
                Back
              </button>
              <button
                className="btn btn-primary flex-1"
                onClick={saveKeyAndNext}
                disabled={!groqKey.trim()}
              >
                {keyStatus === "ok" ? "Next" : "Next (skip test)"}
              </button>
            </div>
          </div>
        )}

        {/* ── Step 3: Knowledge base ── */}
        {step === 3 && (
          <div className="flex flex-col gap-5 w-full">
            <div>
              <h2
                className="text-xl font-normal mb-2"
                style={{ fontFamily: "var(--font-serif)", color: "var(--ink)" }}
              >
                Knowledge base
              </h2>
              <p className="text-sm leading-relaxed" style={{ color: "var(--muted)" }}>
                Point MeetAI at a folder of markdown or text files. Relevant notes surface
                automatically while you talk. You can change this later in Settings.
              </p>
            </div>

            <div className="flex gap-2">
              <input
                className="input flex-1 text-sm"
                placeholder="No folder selected"
                value={kbFolder}
                readOnly
                style={{ fontFamily: "var(--font-mono)", color: kbFolder ? "var(--ink)" : "var(--muted)" }}
              />
              <button className="btn" onClick={chooseFolder}>
                Choose…
              </button>
            </div>

            <div className="flex flex-col gap-2">
              <button className="btn btn-primary w-full" onClick={finish}>
                {kbFolder ? "Finish & start indexing" : "Finish"}
              </button>
              {!kbFolder && (
                <button
                  className="text-xs text-center"
                  style={{ color: "var(--muted)", background: "none", border: "none", cursor: "pointer" }}
                  onClick={finish}
                >
                  Skip for now
                </button>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
