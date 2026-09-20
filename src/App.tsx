import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useApp, saveReadingPos, loadReadingPos } from "./state/store";
import {
  closeDocument,
  isApiError,
  openDocument,
  saveDocument,
  undoDocument,
  canUndo,
  redoDocument,
  canRedo,
  getBookmarks,
} from "./lib/ipc";
import type { BookmarkNode } from "./lib/ipc";
import Toolbar from "./components/Toolbar";
import Rail from "./components/Rail";
import Canvas from "./components/Canvas";
import ThumbnailPanel from "./components/ThumbnailPanel";
import SearchPanel from "./components/SearchPanel";
import TaskPanel from "./components/TaskPanel";
import StatusBar from "./components/StatusBar";
import "./index.css";

function BookmarkTree({
  nodes,
  onJump,
}: {
  nodes: BookmarkNode[];
  onJump: (page: number) => void;
}) {
  if (nodes.length === 0) {
    return <div style={{ padding: 16, color: "var(--fg-dim)" }}>文档没有书签</div>;
  }
  return (
    <div className="bookmark-tree">
      {nodes.map((node, i) => (
        <div key={i}>
          <div
            className="bookmark-item"
            style={{ paddingLeft: 8 + node.level * 16 }}
            onClick={() => onJump(node.pageIndex)}
          >
            {node.title || "（无标题）"}
          </div>
          {node.children.length > 0 && (
            <BookmarkTree nodes={node.children} onJump={onJump} />
          )}
        </div>
      ))}
    </div>
  );
}

function LeftPanel() {
  const leftTab = useApp((s) => s.leftTab);
  const setLeftTab = useApp((s) => s.setLeftTab);
  const hasDoc = useApp((s) => s.docId !== null);
  const docId = useApp((s) => s.docId);
  const bookmarks = useApp((s) => s.bookmarks);
  const bookmarksLoading = useApp((s) => s.bookmarksLoading);
  const setBookmarks = useApp((s) => s.setBookmarks);
  const setBookmarksLoading = useApp((s) => s.setBookmarksLoading);
  const jumpToPage = useApp((s) => s.jumpToPage);
  const errorToast = useApp((s) => s.errorToast);

  useEffect(() => {
    if (docId === null || leftTab !== "bookmarks") return;
    let cancelled = false;
    setBookmarksLoading(true);
    getBookmarks(docId)
      .then((b) => {
        if (!cancelled) setBookmarks(b);
      })
      .catch((e) => {
        if (!cancelled) errorToast(e);
      })
      .finally(() => {
        if (!cancelled) setBookmarksLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [docId, leftTab]);

  return (
    <aside className="left">
      <div className="tabs">
        <button
          className={leftTab === "thumbnails" ? "active" : ""}
          onClick={() => setLeftTab("thumbnails")}
        >
          缩略图
        </button>
        <button
          className={leftTab === "bookmarks" ? "active" : ""}
          onClick={() => setLeftTab("bookmarks")}
        >
          书签
        </button>
      </div>
      {leftTab === "thumbnails" ? (
        hasDoc ? (
          <ThumbnailPanel />
        ) : (
          <div style={{ padding: 16, color: "var(--fg-dim)" }}>打开文档后显示缩略图</div>
        )
      ) : hasDoc ? (
        bookmarksLoading ? (
          <div style={{ padding: 16, color: "var(--fg-dim)" }}>加载中…</div>
        ) : (
          <BookmarkTree nodes={bookmarks} onJump={(p) => jumpToPage(p, true)} />
        )
      ) : (
        <div style={{ padding: 16, color: "var(--fg-dim)" }}>打开文档后显示书签</div>
      )}
    </aside>
  );
}

function PasswordDialog({
  path,
  onDone,
}: {
  path: string;
  onDone: () => void;
}) {
  const [pwd, setPwd] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const setDoc = useApp((s) => s.setDoc);
  const pushToast = useApp((s) => s.pushToast);

  const submit = async () => {
    setBusy(true);
    setErr("");
    try {
      const info = await openDocument(path, pwd);
      setDoc(info, path);
      const pos = loadReadingPos(info.fileName, info.pageCount);
      if (pos) useApp.getState().jumpToPage(pos.page);
      pushToast("info", `已打开 ${info.fileName}（${info.pageCount} 页）`);
      onDone();
    } catch (e) {
      setErr(isApiError(e) ? (e.code === "password" ? "密码错误，请重试" : e.message) : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,.4)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 200,
      }}
      onClick={onDone}
    >
      <div
        style={{
          background: "var(--bg-panel)",
          borderRadius: 8,
          padding: 20,
          width: 340,
          boxShadow: "var(--page-shadow)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <h3 style={{ marginBottom: 10 }}>该文档已加密</h3>
        <p style={{ color: "var(--fg-dim)", marginBottom: 10 }}>请输入打开密码：</p>
        <input
          type="password"
          autoFocus
          style={{ width: "100%" }}
          value={pwd}
          onChange={(e) => setPwd(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
        />
        {err && <p style={{ color: "var(--danger)", marginTop: 8 }}>{err}</p>}
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 14 }}>
          <button onClick={onDone}>取消</button>
          <button className="btn-primary" disabled={busy || !pwd} onClick={submit}>
            {busy ? "验证中…" : "打开"}
          </button>
        </div>
      </div>
    </div>
  );
}

export default function App() {
  const theme = useApp((s) => s.theme);
  const docId = useApp((s) => s.docId);
  const leftVisible = useApp((s) => s.leftVisible);
  const fileName = useApp((s) => s.fileName);
  const pageCount = useApp((s) => s.pageCount);
  const loading = useApp((s) => s.loading);
  const toasts = useApp((s) => s.toasts);
  const dismissToast = useApp((s) => s.dismissToast);

  const [pwdPath, setPwdPath] = useState<string | null>(null);

  // 主题同步到 <html data-theme>
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  // 跟随系统深浅色
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (e: MediaQueryListEvent) =>
      useApp.getState().setTheme(e.matches ? "dark" : "light");
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  const doOpen = useCallback(
    async (path: string) => {
      const st = useApp.getState();
      st.setLoading(true);
      try {
        const info = await openDocument(path);
        st.setDoc(info, path);
        const pos = loadReadingPos(info.fileName, info.pageCount);
        if (pos) st.jumpToPage(pos.page);
        st.pushToast("info", `已打开 ${info.fileName}（${info.pageCount} 页）`);
        canUndo(info.docId).then(st.setCanUndo).catch(() => {});
        canRedo(info.docId).then(st.setCanRedo).catch(() => {});
      } catch (e) {
        if (isApiError(e) && e.code === "password") {
          setPwdPath(path);
        } else {
          st.errorToast(e);
        }
      } finally {
        useApp.getState().setLoading(false);
      }
    },
    [],
  );

  const onOpenFile = useCallback(async () => {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF 文档", extensions: ["pdf"] }],
    });
    if (typeof picked === "string") doOpen(picked);
  }, [doOpen]);

  const onSave = useCallback(async () => {
    const st = useApp.getState();
    if (st.docId === null) return;
    try {
      await saveDocument(st.docId);
      st.markDirty(false);
      st.pushToast("info", "已保存");
    } catch (e) {
      st.errorToast(e);
    }
  }, []);

  const onUndo = useCallback(async () => {
    const st = useApp.getState();
    if (st.docId === null) return;
    try {
      const info = await undoDocument(st.docId);
      st.updatePages(info);
      st.setCanUndo(await canUndo(st.docId));
      st.setCanRedo(await canRedo(st.docId));
      st.pushToast("info", "已撤销");
    } catch (e) {
      st.errorToast(e);
    }
  }, []);

  const onRedo = useCallback(async () => {
    const st = useApp.getState();
    if (st.docId === null) return;
    try {
      const info = await redoDocument(st.docId);
      st.updatePages(info);
      st.setCanUndo(await canUndo(st.docId));
      st.setCanRedo(await canRedo(st.docId));
      st.pushToast("info", "已重做");
    } catch (e) {
      st.errorToast(e);
    }
  }, []);

  const zoomBy = useCallback((factor: number) => {
    const st = useApp.getState();
    st.setScale(st.scale * factor);
  }, []);

  const resetZoom = useCallback(() => {
    useApp.getState().setFitMode("width");
  }, []);

  // 快捷键
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      const mod = e.ctrlKey || e.metaKey;
      const st = useApp.getState();
      if (mod && e.key.toLowerCase() === "o") {
        e.preventDefault();
        onOpenFile();
      } else if (mod && e.key.toLowerCase() === "s") {
        e.preventDefault();
        onSave();
      } else if (mod && e.key.toLowerCase() === "f") {
        e.preventDefault();
        st.setSearchOpen(true);
      } else if (mod && e.key.toLowerCase() === "z") {
        e.preventDefault();
        onUndo();
      } else if (mod && (e.key.toLowerCase() === "y" || (e.shiftKey && e.key.toLowerCase() === "z"))) {
        // Ctrl+Y 或 Ctrl+Shift+Z 都触发重做
        e.preventDefault();
        onRedo();
      } else if (mod && e.key === "0") {
        e.preventDefault();
        resetZoom();
      } else if (mod && (e.key === "=" || e.key === "+")) {
        e.preventDefault();
        zoomBy(1.2);
      } else if (mod && e.key === "-") {
        e.preventDefault();
        zoomBy(1 / 1.2);
      } else if (e.key === "PageDown") {
        e.preventDefault();
        st.jumpToPage(st.currentPage + 1);
      } else if (e.key === "PageUp") {
        e.preventDefault();
        st.jumpToPage(st.currentPage - 1);
      } else if (e.key === "Home") {
        e.preventDefault();
        st.jumpToPage(0);
      } else if (e.key === "End") {
        e.preventDefault();
        st.jumpToPage(Math.max(0, st.pageCount - 1));
      } else if (e.key === "Escape") {
        // 关闭搜索/任务面板
        st.setSearchOpen(false);
        st.closeTask();
      } else if (e.key === "F3") {
        e.preventDefault();
        st.setSearchOpen(true);
      } else if (mod && e.key === "g") {
        // 切换左面板可见性
        e.preventDefault();
        st.toggleLeft();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onOpenFile, onSave, onUndo, onRedo, zoomBy, resetZoom]);

  // 阅读位置记忆（节流保存）
  useEffect(() => {
    if (docId === null) return;
    const t = setInterval(() => {
      const s = useApp.getState();
      saveReadingPos(s.fileName, s.pageCount, s.currentPage, s.scale);
    }, 2000);
    return () => {
      clearInterval(t);
      const s = useApp.getState();
      saveReadingPos(s.fileName, s.pageCount, s.currentPage, s.scale);
    };
  }, [docId, fileName, pageCount]);

  // 关闭文档释放内存
  useEffect(() => {
    return () => {
      if (docId !== null) closeDocument(docId).catch(() => {});
    };
  }, [docId]);

  return (
    <div className={`shell${leftVisible ? "" : " no-left"}`}>
      <Toolbar onOpenFile={onOpenFile} onUndo={onUndo} onRedo={onRedo} />
      <Rail />
      <LeftPanel />
      {docId !== null ? (
        <Canvas />
      ) : (
        <div className="canvas-wrap">
          <div className="empty">
            <span className="big">📄</span>
            <span>打开一个 PDF 文件开始阅读</span>
            <button onClick={onOpenFile} disabled={loading}>
              {loading ? "加载中…" : "打开文件"}
            </button>
            <span style={{ fontSize: 12 }}>Ctrl+O 打开 · Ctrl+F 搜索 · Ctrl+滚轮缩放</span>
          </div>
        </div>
      )}
      <TaskPanel />
      <SearchPanel />
      <StatusBar />
      {pwdPath && <PasswordDialog path={pwdPath} onDone={() => setPwdPath(null)} />}
      <div className="toasts">
        {toasts.map((t) => (
          <div key={t.id} className={`toast ${t.kind}`} onClick={() => dismissToast(t.id)}>
            {t.message}
          </div>
        ))}
      </div>
    </div>
  );
}
