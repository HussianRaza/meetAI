import { create } from "zustand";
import type { TranscriptSegment } from "../ipc";

export interface SessionEntry extends TranscriptSegment {
  id: string; // client-side key
}

export interface NudgeCard {
  id: string;
  file_path: string;
  breadcrumb: string;
  snippet: string;
  score: number;
  suggestion: string | null;
}

interface SessionStore {
  isRecording: boolean;
  meetingId: string | null;
  title: string;
  startedAt: number | null;
  transcript: SessionEntry[];
  nudgeCards: NudgeCard[]; // most-recent first, max 3

  setTitle: (t: string) => void;
  startRecording: (meetingId: string, title: string) => void;
  stopRecording: () => void;
  addSegment: (seg: TranscriptSegment) => void;
  addNudgeCard: (card: NudgeCard) => void;
  reset: () => void;
}

let _counter = 0;

export const useSessionStore = create<SessionStore>((set) => ({
  isRecording: false,
  meetingId: null,
  title: "",
  startedAt: null,
  transcript: [],
  nudgeCards: [],

  setTitle: (t) => set({ title: t }),

  startRecording: (meetingId, title) =>
    set({
      isRecording: true,
      meetingId,
      title,
      startedAt: Date.now(),
      transcript: [],
      nudgeCards: [],
    }),

  stopRecording: () =>
    set({ isRecording: false, meetingId: null, startedAt: null }),

  addSegment: (seg) =>
    set((s) => ({
      transcript: [...s.transcript, { ...seg, id: `seg-${++_counter}` }],
    })),

  // Prepend new card (newest first), keep max 3
  addNudgeCard: (card) =>
    set((s) => ({
      nudgeCards: [card, ...s.nudgeCards].slice(0, 3),
    })),

  reset: () =>
    set({
      isRecording: false,
      meetingId: null,
      title: "",
      startedAt: null,
      transcript: [],
      nudgeCards: [],
    }),
}));
