import { useRef, useState } from "react";
import { ipc } from "../ipc";
import type { ChatResponse, SourceMeeting } from "../ipc";
import Sidebar from "../components/Sidebar";

// ── Source citation ───────────────────────────────────────────────────────────

function SourceTag({ source }: { source: SourceMeeting }) {
  const date = new Date(source.started_at).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
  return (
    <span
      className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] border border-[var(--border)]"
      style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
    >
      {source.title} · {date}
    </span>
  );
}

// ── Message bubble ────────────────────────────────────────────────────────────

interface Message {
  id: number;
  role: "user" | "assistant";
  text: string;
  sources?: SourceMeeting[];
}

function MessageBubble({ msg }: { msg: Message }) {
  if (msg.role === "user") {
    return (
      <div className="flex justify-end">
        <div
          className="max-w-lg px-4 py-2.5 rounded-2xl rounded-tr-sm text-sm"
          style={{
            background: "var(--ink)",
            color: "var(--paper)",
            lineHeight: 1.6,
          }}
        >
          {msg.text}
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2 max-w-2xl">
      <div
        className="px-4 py-3 rounded-2xl rounded-tl-sm text-sm"
        style={{
          background: "var(--surface)",
          border: "1px solid var(--border)",
          color: "var(--ink)",
          lineHeight: 1.7,
          whiteSpace: "pre-wrap",
        }}
      >
        {msg.text}
      </div>
      {msg.sources && msg.sources.length > 0 && (
        <div className="flex flex-wrap gap-1.5 px-1">
          {msg.sources.map((s) => (
            <SourceTag key={s.id} source={s} />
          ))}
        </div>
      )}
    </div>
  );
}

// ── Chat screen ───────────────────────────────────────────────────────────────

export default function Chat() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);
  let msgId = useRef(0);

  const addMessage = (msg: Omit<Message, "id">): Message => {
    const full: Message = { ...msg, id: ++msgId.current };
    setMessages((prev) => [...prev, full]);
    setTimeout(
      () => bottomRef.current?.scrollIntoView({ behavior: "smooth" }),
      50
    );
    return full;
  };

  const handleSend = async () => {
    const q = input.trim();
    if (!q || loading) return;
    setInput("");

    addMessage({ role: "user", text: q });
    setLoading(true);

    try {
      const resp: ChatResponse = await ipc.chatQuery(q);
      addMessage({ role: "assistant", text: resp.answer, sources: resp.sources });
    } catch (e) {
      addMessage({
        role: "assistant",
        text: `Error: ${String(e)}`,
        sources: [],
      });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex h-screen">
      <Sidebar groqOk={null} />
      <div className="flex flex-col flex-1 min-w-0">
        {/* Header */}
        <header
          className="px-6 py-4 border-b border-[var(--border)]"
          style={{ background: "var(--surface)" }}
        >
          <div
            className="text-lg font-semibold"
            style={{ fontFamily: "var(--font-serif)", color: "var(--ink)" }}
          >
            Chat with Meetings
          </div>
          <div
            className="text-xs mt-0.5"
            style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
          >
            Ask questions across all your recorded meetings
          </div>
        </header>

        {/* Messages */}
        <div className="flex-1 overflow-y-auto px-6 py-4 flex flex-col gap-4">
          {messages.length === 0 && (
            <div className="flex flex-col gap-3 mt-10 items-center">
              <div
                className="text-sm"
                style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
              >
                Ask anything about your meetings
              </div>
              <div className="flex flex-wrap gap-2 justify-center">
                {[
                  "What did we decide last week?",
                  "What action items were discussed?",
                  "Summarise the pricing discussion",
                ].map((s) => (
                  <button
                    key={s}
                    className="btn btn-sm btn-outline text-xs"
                    onClick={() => setInput(s)}
                  >
                    {s}
                  </button>
                ))}
              </div>
            </div>
          )}
          {messages.map((m) => (
            <MessageBubble key={m.id} msg={m} />
          ))}
          {loading && (
            <div
              className="text-xs animate-pulse"
              style={{ color: "var(--muted)", fontFamily: "var(--font-mono)" }}
            >
              Thinking…
            </div>
          )}
          <div ref={bottomRef} />
        </div>

        {/* Input bar */}
        <div
          className="px-4 pb-4 pt-2 border-t border-[var(--border)]"
          style={{ background: "var(--surface)" }}
        >
          <div className="flex items-end gap-2">
            <textarea
              className="input flex-1 resize-none text-sm"
              rows={2}
              placeholder="Ask about your meetings…"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  handleSend();
                }
              }}
              disabled={loading}
            />
            <button
              className="btn btn-primary"
              onClick={handleSend}
              disabled={!input.trim() || loading}
            >
              Send
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
