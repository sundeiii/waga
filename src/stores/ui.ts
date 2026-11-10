import { create } from "zustand";
import type { Page, ServerInput, ServerView } from "@/types";

export interface Toast {
  id: number;
  tone: "info" | "success" | "warning" | "danger";
  text: string;
}

interface UiStore {
  page: Page;
  search: string;
  serverDialog: { open: boolean; editing?: ServerView; prefill?: Partial<ServerInput>; connectAfterSave?: boolean };
  quickConnectOpen: boolean;
  confirm: { title: string; body: string; confirmLabel: string; danger?: boolean; onConfirm: () => Promise<void> | void; onCancel?: () => void } | null;
  toasts: Toast[];
  /** Server to preselect when opening the SFTP page. */
  sftpTarget: string | null;
  openSftp: (serverId: string) => void;
  setPage: (p: Page) => void;
  setSearch: (s: string) => void;
  openServerDialog: (opts?: { editing?: ServerView; prefill?: Partial<ServerInput>; connectAfterSave?: boolean }) => void;
  closeServerDialog: () => void;
  setQuickConnect: (open: boolean) => void;
  askConfirm: (c: NonNullable<UiStore["confirm"]>) => void;
  closeConfirm: () => void;
  /** Resolves true on confirm, false on cancel. */
  confirmAsync: (c: { title: string; body: string; confirmLabel: string; danger?: boolean }) => Promise<boolean>;
  toast: (text: string, tone?: Toast["tone"]) => void;
  dismissToast: (id: number) => void;
}

let toastId = 0;

export const useUi = create<UiStore>((set, get) => ({
  page: "overview",
  search: "",
  serverDialog: { open: false },
  quickConnectOpen: false,
  confirm: null,
  toasts: [],
  sftpTarget: null,
  openSftp: (sftpTarget) => set({ sftpTarget, page: "sftp" }),
  setPage: (page) => set({ page }),
  setSearch: (search) => set({ search }),
  openServerDialog: (opts) => set({ serverDialog: { open: true, ...opts } }),
  closeServerDialog: () => set({ serverDialog: { open: false } }),
  setQuickConnect: (quickConnectOpen) => set({ quickConnectOpen }),
  askConfirm: (confirm) => set({ confirm }),
  closeConfirm: () => set({ confirm: null }),
  confirmAsync: (c) =>
    new Promise<boolean>((resolve) => {
      set({
        confirm: {
          ...c,
          onConfirm: () => resolve(true),
          onCancel: () => resolve(false),
        },
      });
    }),
  toast: (text, tone = "info") => {
    const id = ++toastId;
    set({ toasts: [...get().toasts, { id, text, tone }] });
    setTimeout(() => get().dismissToast(id), 4200);
  },
  dismissToast: (id) => set({ toasts: get().toasts.filter((t) => t.id !== id) }),
}));
