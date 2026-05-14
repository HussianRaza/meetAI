import { create } from "zustand";
import { ipc, type Settings } from "../ipc";

interface SettingsStore {
  settings: Settings | null;
  loaded: boolean;
  load: () => Promise<void>;
  set: (key: keyof Settings, value: string) => Promise<void>;
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
  settings: null,
  loaded: false,

  load: async () => {
    const settings = await ipc.settingsGet();
    set({ settings, loaded: true });
  },

  set: async (key, value) => {
    await ipc.settingsSet(key, value);
    const current = get().settings;
    if (current) {
      set({ settings: { ...current, [key]: value } });
    }
  },
}));
