import { NavLink } from "react-router-dom";
import { useSessionStore } from "../stores/session";

interface Props {
  groqOk: boolean | null;
}

export default function Sidebar({ groqOk }: Props) {
  const { isRecording, title } = useSessionStore();

  const navCls = ({ isActive }: { isActive: boolean }) =>
    `flex items-center justify-between px-3 py-1.5 rounded text-sm cursor-pointer no-underline transition-colors ${
      isActive
        ? "bg-[var(--ink)] text-[var(--paper)]"
        : "text-[var(--ink)] hover:bg-[var(--surface-subtle)]"
    }`;

  return (
    <aside
      className="flex flex-col justify-between w-52 shrink-0 border-r border-[var(--border)] bg-[var(--surface)] px-4 py-5"
      style={{ minHeight: "100vh" }}
    >
      <div>
        <div className="mb-5">
          <div className="flex items-center gap-2 font-semibold text-base text-[var(--ink)]">
            <span
              className="w-5 h-5 rounded-sm bg-[var(--ink)] inline-block"
              aria-hidden
            />
            MeetAI
          </div>
          <div
            className="mt-0.5 text-[11px] tracking-widest uppercase"
            style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
          >
            v0.1 · local
          </div>
        </div>

        <nav className="flex flex-col gap-0.5">
          <div
            className="mt-1 mb-1 text-[10px] font-semibold tracking-widest uppercase"
            style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
          >
            Workspace
          </div>
          <NavLink to="/library" className={navCls}>
            <span>Library</span>
          </NavLink>
          <NavLink to="/chat" className={navCls}>
            Chat
          </NavLink>
          <NavLink to="/settings" className={navCls}>
            Settings
          </NavLink>

          <div
            className="mt-3 mb-1 text-[10px] font-semibold tracking-widest uppercase"
            style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
          >
            Now
          </div>
          {isRecording ? (
            <NavLink to="/live" className={navCls}>
              <span className="flex items-center gap-2">
                <span
                  className="inline-block w-1.5 h-1.5 rounded-full animate-pulse"
                  style={{ background: "var(--red)" }}
                />
                {title || "Recording"}
              </span>
            </NavLink>
          ) : (
            <NavLink to="/live" className={navCls}>
              + New Meeting
            </NavLink>
          )}
        </nav>
      </div>

      <div
        className="flex flex-col gap-1 text-[11px] pt-4 border-t border-[var(--border)]"
        style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
      >
        <div className="flex items-center gap-2">
          <span
            className={`inline-block w-1.5 h-1.5 rounded-full ${
              groqOk === true
                ? "bg-[var(--green)]"
                : groqOk === false
                ? "bg-[var(--red)]"
                : "bg-[var(--muted)]"
            }`}
          />
          Groq · {groqOk === true ? "ok" : groqOk === false ? "error" : "—"}
        </div>
        <div>KB · —</div>
        <div>Whisper · —</div>
      </div>
    </aside>
  );
}
