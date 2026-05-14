import { create } from "zustand";
import type { TranscriptSegment } from "../ipc";

export interface SessionEntry extends TranscriptSegment {
  id: string; // client-side id for React key
}

interface SessionStore {
  isRecording: boolean;
  meetingId: string | null;
  title: string;
  startedAt: number | null;
  transcript: SessionEntry[];

  // Actions
  setTitle: (t: string) => void;
  startRecording: (meetingId: string, title: string) => void;
  stopRecording: () => void;
  addSegment: (seg: TranscriptSegment) => void;
  reset: () => void;
}

let _counter = 0;

export const useSessionStore = create<SessionStore>((set) => ({
  isRecording: false,
  meetingId: null,
  title: "",
  startedAt: null,
  transcript: [],

  setTitle: (t) => set({ title: t }),

  startRecording: (meetingId, title) =>
    set({
      isRecording: true,
      meetingId,
      title,
      startedAt: Date.now(),
      transcript: [],
    }),

  stopRecording: () =>
    set({ isRecording: false, meetingId: null, startedAt: null }),

  addSegment: (seg) =>
    set((s) => ({
      transcript: [
        ...s.transcript,
        { ...seg, id: `seg-${++_counter}` },
      ],
    })),

  reset: () =>
    set({
      isRecording: false,
      meetingId: null,
      title: "",
      startedAt: null,
      transcript: [],
    }),
}));
