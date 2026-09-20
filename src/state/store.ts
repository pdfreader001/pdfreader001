import { create } from "zustand";
import type { DocumentInfo, PageInfo, BookmarkNode } from "../lib/ipc";
import { isApiError } from "../lib/ipc";
import { pageCache, thumbCache } from "../lib/bitmapCache";

export type ViewMode = "continuous" | "single" | "dual";
export type FitMode = "none" | "width" | "page";
export type LeftTab = "thumbnails" | "bookmarks";
export type TaskId = "merge" | "split" | "watermark" | "edit" | "security" | "export" | null;

export interface SearchHit {
  pageIndex: number;
  /** 命中在页面纯文本中的字符偏移 */
  offset: number;
  snippet: string;
}

export interface Toast {
  id: number;
  kind: "info" | "error";
  message: string;
}

interface AppState {
  // 文档
  docId: number | null;
  fileName: string;
  filePath: string | null;
  pageCount: number;
  pages: PageInfo[];
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  loading: boolean;
  /** 页面内容版本号（水印/旋转等修改后自增，触发画布重渲） */
  renderRevision: number;

  // 视图
  viewMode: ViewMode;
  fitMode: FitMode;
  scale: number;
  currentPage: number; // 0-based
  scrollTop: number;

  // 面板
  leftTab: LeftTab;
  leftVisible: boolean;
  task: TaskId;
  searchOpen: boolean;
  helpOpen: boolean;

  // 搜索
  searchQuery: string;
  searchHits: SearchHit[];
  searchActive: number; // 当前 F3 定位的命中索引，-1 表示无
  searching: boolean;

  // 主题
  theme: "light" | "dark";

  // 语言
  locale: "zh" | "en";

  // 缩略图选择
  selectedPages: Set<number>;
  thumbFocus: number;

  // 书签
  bookmarks: BookmarkNode[];
  bookmarksLoading: boolean;

  toasts: Toast[];
  jumpTarget: { page: number; nonce: number };
  flashTarget: { page: number; nonce: number };

  // actions
  jumpToPage: (page: number, flash?: boolean) => void;
  updatePages: (info: DocumentInfo) => void;
  setDoc: (info: DocumentInfo, path: string | null) => void;
  clearDoc: () => void;
  setViewMode: (m: ViewMode) => void;
  setScale: (s: number) => void;
  applyFitScale: (s: number) => void;
  setFitMode: (f: FitMode) => void;
  setCurrentPage: (p: number) => void;
  setScrollTop: (t: number) => void;
  setLeftTab: (t: LeftTab) => void;
  toggleLeft: () => void;
  openTask: (t: TaskId) => void;
  closeTask: () => void;
  setSearchOpen: (b: boolean) => void;
  setHelpOpen: (b: boolean) => void;
  setSearch: (query: string, hits: SearchHit[]) => void;
  setSearchActive: (i: number) => void;
  setSearching: (b: boolean) => void;
  setTheme: (t: "light" | "dark") => void;
  setLocale: (l: "zh" | "en") => void;
  toggleSelect: (page: number, ctrl: boolean, shift: boolean) => void;
  clearSelection: () => void;
  markDirty: (b: boolean) => void;
  setCanUndo: (b: boolean) => void;
  setCanRedo: (b: boolean) => void;
  setLoading: (b: boolean) => void;
  setBookmarks: (b: BookmarkNode[]) => void;
  setBookmarksLoading: (b: boolean) => void;
  pushToast: (kind: Toast["kind"], message: string) => void;
  dismissToast: (id: number) => void;
  errorToast: (e: unknown) => void;
}

/** 阅读位置持久化：按文件名+页数作为指纹 */
const POS_PREFIX = "pdfe:pos:";
export function saveReadingPos(fileName: string, pageCount: number, page: number, scale: number) {
  try {
    localStorage.setItem(
      POS_PREFIX + `${fileName}:${pageCount}`,
      JSON.stringify({ page, scale }),
    );
  } catch {
    /* 忽略存储失败 */
  }
}
export function loadReadingPos(
  fileName: string,
  pageCount: number,
): { page: number; scale: number } | null {
  try {
    const raw = localStorage.getItem(POS_PREFIX + `${fileName}:${pageCount}`);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

let toastSeq = 0;

export const useApp = create<AppState>((set, get) => ({
  docId: null,
  fileName: "",
  filePath: null,
  pageCount: 0,
  pages: [],
  dirty: false,
  canUndo: false,
  canRedo: false,
  loading: false,
  renderRevision: 0,

  viewMode: "continuous",
  fitMode: "width",
  scale: 1,
  currentPage: 0,
  scrollTop: 0,

  leftTab: "thumbnails",
  leftVisible: true,
  task: null,
  searchOpen: false,
  helpOpen: false,

  searchQuery: "",
  searchHits: [],
  searchActive: -1,
  searching: false,

  theme: window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light",

  locale: (localStorage.getItem("pdfe:locale") as "zh" | "en") || "zh",

  selectedPages: new Set(),
  thumbFocus: -1,

  bookmarks: [],
  bookmarksLoading: false,

  toasts: [],
  jumpTarget: { page: 0, nonce: 0 },
  flashTarget: { page: -1, nonce: 0 },

  jumpToPage: (page, flash) =>
    set((s) => {
      const p = Math.max(0, Math.min(page, Math.max(0, s.pageCount - 1)));
      const base = { currentPage: p };
      if (flash) {
        return { ...base, jumpTarget: { page: p, nonce: s.jumpTarget.nonce + 1 }, flashTarget: { page: p, nonce: s.flashTarget.nonce + 1 } };
      }
      return { ...base, jumpTarget: { page: p, nonce: s.jumpTarget.nonce + 1 } };
    }),
  updatePages: (info) => {
    pageCache.clear();
    thumbCache.clear();
    set((s) => ({
      pageCount: info.pageCount,
      pages: info.pages,
      dirty: true,
      renderRevision: s.renderRevision + 1,
    }));
  },

  setDoc: (info, path) => {
    pageCache.clear();
    thumbCache.clear();
    set({
      docId: info.docId,
      fileName: info.fileName,
      filePath: path,
      pageCount: info.pageCount,
      pages: info.pages,
      dirty: false,
      canUndo: false,
      canRedo: false,
      currentPage: 0,
      scrollTop: 0,
      selectedPages: new Set(),
      thumbFocus: -1,
      searchHits: [],
      searchActive: -1,
      searchQuery: "",
    });
  },
  clearDoc: () =>
    set({
      docId: null,
      fileName: "",
      filePath: null,
      pageCount: 0,
      pages: [],
      dirty: false,
      canUndo: false,
      selectedPages: new Set(),
    }),
  setViewMode: (m) => set({ viewMode: m, fitMode: m === "single" ? "page" : "width" }),
  setScale: (s) => set({ scale: Math.min(8, Math.max(0.1, s)), fitMode: "none" }),
  applyFitScale: (s) => set({ scale: Math.min(8, Math.max(0.1, s)) }),
  setFitMode: (f) => set({ fitMode: f }),
  setCurrentPage: (p) => set({ currentPage: p }),
  setScrollTop: (t) => set({ scrollTop: t }),
  setLeftTab: (t) => set({ leftTab: t, leftVisible: true }),
  toggleLeft: () => set((s) => ({ leftVisible: !s.leftVisible })),
  openTask: (t) => set({ task: t, searchOpen: false }),
  closeTask: () => set({ task: null }),
  setSearchOpen: (b) => set({ searchOpen: b }),
  setHelpOpen: (b) => set({ helpOpen: b }),
  setSearch: (query, hits) => set({ searchQuery: query, searchHits: hits, searchActive: hits.length ? 0 : -1 }),
  setSearchActive: (i) => set({ searchActive: i }),
  setSearching: (b) => set({ searching: b }),
  setTheme: (t) => set({ theme: t }),
  setLocale: (l) => {
    localStorage.setItem("pdfe:locale", l);
    set({ locale: l });
  },
  toggleSelect: (page, ctrl, shift) => {
    const { selectedPages, thumbFocus } = get();
    const next = new Set(selectedPages);
    if (shift && thumbFocus >= 0) {
      const [a, b] = [Math.min(thumbFocus, page), Math.max(thumbFocus, page)];
      for (let i = a; i <= b; i++) next.add(i);
    } else if (ctrl) {
      if (next.has(page)) next.delete(page);
      else next.add(page);
    } else {
      next.clear();
      next.add(page);
    }
    set({ selectedPages: next, thumbFocus: page });
  },
  clearSelection: () => set({ selectedPages: new Set(), thumbFocus: -1 }),
  markDirty: (b) => set({ dirty: b }),
  setCanUndo: (b) => set({ canUndo: b }),
  setCanRedo: (b) => set({ canRedo: b }),
  setLoading: (b) => set({ loading: b }),
  setBookmarks: (b) => set({ bookmarks: b }),
  setBookmarksLoading: (b) => set({ bookmarksLoading: b }),
  pushToast: (kind, message) => {
    const id = ++toastSeq;
    set((s) => ({ toasts: [...s.toasts, { id, kind, message }] }));
    setTimeout(() => {
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
    }, kind === "error" ? 5000 : 2500);
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
  errorToast: (e) => {
    const msg = isApiError(e) ? e.message : e instanceof Error ? e.message : String(e);
    get().pushToast("error", msg);
  },
}));
