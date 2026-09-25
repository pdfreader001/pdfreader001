import { useCallback, useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useApp } from "../../state/store";
import { translateError, useT } from "../../i18n";
import { inspectMergeSources, mergeDocuments, type MergeSourceInfo } from "../../lib/ipc";

interface MergeItem {
  id: number;
  path: string;
  name: string;
  ranges: string;
}

function baseName(p: string): string {
  return p.split(/[\\/]/).pop() || p;
}

function moveItem<T>(arr: T[], from: number, to: number): T[] {
  const next = [...arr];
  const [it] = next.splice(from, 1);
  next.splice(to, 0, it);
  return next;
}

export default function MergePanel() {
  const [items, setItems] = useState<MergeItem[]>([]);
  const [outputPath, setOutputPath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // 预览按 id 存放，重排/删除时不会串位；陈旧结果由 seq 丢弃。
  const [previewById, setPreviewById] = useState<Record<number, MergeSourceInfo>>({});
  const [dropActive, setDropActive] = useState(false);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [dragOver, setDragOver] = useState<{ index: number; pos: "before" | "after" } | null>(null);
  const nextIdRef = useRef(1);
  const previewSeqRef = useRef(0);
  const setDoc = useApp((s) => s.setDoc);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const closeTask = useApp((s) => s.closeTask);
  const locale = useApp((s) => s.locale);
  const t = useT();

  const makeItems = useCallback(
    (paths: string[]): MergeItem[] =>
      paths.map((p) => ({ id: nextIdRef.current++, path: p, name: baseName(p), ranges: "" })),
    [],
  );

  const addPaths = useCallback(
    (paths: string[]) => {
      const pdfs = paths.filter((p) => p.toLowerCase().endsWith(".pdf"));
      if (pdfs.length === 0) return;
      setItems((prev) => [...prev, ...makeItems(pdfs)]);
    },
    [makeItems],
  );

  // 系统级文件拖入（Tauri v2 原生接管 OS 拖放，HTML5 drop 收不到文件）
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type === "enter" || e.payload.type === "over") {
          setDropActive(true);
        } else if (e.payload.type === "leave") {
          setDropActive(false);
        } else {
          setDropActive(false);
          addPaths(e.payload.paths);
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [addPaths]);

  // 合并预览：输入变更后防抖请求，逐源显示页数信息。
  useEffect(() => {
    if (items.length === 0) {
      previewSeqRef.current += 1;
      setPreviewById({});
      return;
    }
    const seq = ++previewSeqRef.current;
    const snapshot = items.map((it) => ({
      id: it.id,
      path: it.path,
      ranges: it.ranges.trim() ? it.ranges : null,
    }));
    const timer = setTimeout(() => {
      inspectMergeSources(snapshot.map(({ path, ranges }) => ({ path, ranges })))
        .then((res) => {
          if (seq !== previewSeqRef.current) return;
          const next: Record<number, MergeSourceInfo> = {};
          snapshot.forEach((s, i) => {
            const info = res[i];
            if (info) next[s.id] = info;
          });
          setPreviewById(next);
        })
        .catch(() => {
          if (seq === previewSeqRef.current) setPreviewById({});
        });
    }, 300);
    return () => clearTimeout(timer);
  }, [items]);

  const addFiles = async () => {
    const picked = await open({
      multiple: true,
      filters: [{ name: t("PDF 文档"), extensions: ["pdf"] }],
    });
    if (Array.isArray(picked)) setItems((prev) => [...prev, ...makeItems(picked)]);
  };

  const pickOutput = async () => {
    const p = await save({
      title: t("选择合并结果保存位置"),
      defaultPath: "merged.pdf",
      filters: [{ name: t("PDF 文档"), extensions: ["pdf"] }],
    });
    if (typeof p === "string") setOutputPath(p);
  };

  const move = (idx: number, dir: -1 | 1) => {
    const j = idx + dir;
    if (j < 0 || j >= items.length) return;
    setItems((prev) => moveItem(prev, idx, j));
  };

  const remove = (idx: number) => {
    setItems((prev) => prev.filter((_, i) => i !== idx));
  };

  const updateRanges = (idx: number, ranges: string) => {
    setItems((prev) => prev.map((it, i) => (i === idx ? { ...it, ranges } : it)));
  };

  const handleDragStart = (e: React.DragEvent, index: number) => {
    setDragIndex(index);
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", String(index));
  };

  const handleDragOver = (e: React.DragEvent, index: number) => {
    if (dragIndex === null) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const midY = rect.top + rect.height / 2;
    setDragOver({ index, pos: e.clientY < midY ? "before" : "after" });
  };

  const handleDrop = (e: React.DragEvent, index: number) => {
    e.preventDefault();
    const from = dragIndex;
    const over = dragOver;
    setDragIndex(null);
    setDragOver(null);
    if (from === null || over === null) return;
    let to = over.pos === "before" ? index : index + 1;
    if (from < to) to -= 1;
    if (to === from) return;
    setItems((prev) => moveItem(prev, from, to));
  };

  const handleDragEnd = () => {
    setDragIndex(null);
    setDragOver(null);
  };

  const infos = items.map((it) => previewById[it.id]);
  const allResolved = infos.every((i) => !!i);
  const hasError = infos.some((i) => !!i?.error);
  const totalSelected = infos.reduce((sum, i) => sum + (i && !i.error ? i.selectedPages : 0), 0);
  const canMerge = !busy && items.length > 0 && allResolved && !hasError && totalSelected > 0;

  const doMerge = async () => {
    if (items.length === 0) return;
    setBusy(true);
    try {
      const sources = items.map((it) => ({
        path: it.path,
        ranges: it.ranges.trim() ? it.ranges : null,
      }));
      const info = await mergeDocuments(sources, outputPath);
      setDoc(info, outputPath);
      pushToast(
        "info",
        outputPath
          ? t("合并完成并已保存：{path}（共 {n} 页）", {
              path: outputPath,
              n: info.pageCount,
            })
          : t("合并完成，共 {n} 页", { n: info.pageCount }),
      );
      closeTask();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="task-body">
      <button className="btn-primary" onClick={addFiles} disabled={busy}>
        {t("➕ 添加文件")}
      </button>
      <div className={`merge-list${dropActive ? " drop-active" : ""}`}>
        {items.length === 0 && (
          <p className="placeholder">{t("点击上方按钮添加要合并的 PDF 文件，或把文件拖到这里")}</p>
        )}
        {items.map((it, idx) => {
          const info = previewById[it.id];
          const cls = `merge-item${
            dragOver?.index === idx && dragOver.pos === "before" ? " drop-before" : ""
          }${dragOver?.index === idx && dragOver.pos === "after" ? " drop-after" : ""}`;
          return (
            <div
              key={it.id}
              className={cls}
              onDragOver={(e) => handleDragOver(e, idx)}
              onDrop={(e) => handleDrop(e, idx)}
            >
              <div className="merge-row">
                <span
                  className="merge-handle"
                  draggable
                  title={t("拖动可调整顺序")}
                  onDragStart={(e) => handleDragStart(e, idx)}
                  onDragEnd={handleDragEnd}
                >
                  ⋮⋮
                </span>
                <span className="merge-idx">{idx + 1}</span>
                <span className="merge-name" title={it.path}>
                  {it.name}
                </span>
                <div className="merge-actions">
                  <button onClick={() => move(idx, -1)} disabled={idx === 0}>
                    ↑
                  </button>
                  <button onClick={() => move(idx, 1)} disabled={idx === items.length - 1}>
                    ↓
                  </button>
                  <button onClick={() => remove(idx)} style={{ color: "var(--danger)" }}>
                    ✕
                  </button>
                </div>
              </div>
              <div className="merge-meta">
                {info?.error ? (
                  <span className="merge-meta-error">
                    {translateError(locale, info.error.code, info.error.args, info.error.message)}
                  </span>
                ) : info ? (
                  t("共 {n} 页 · 选用 {k} 页", {
                    n: info.totalPages,
                    k: info.selectedPages,
                  })
                ) : (
                  t("读取中…")
                )}
              </div>
              <input
                type="text"
                placeholder={t("页码范围，如 1,3,5-7（留空为全部页）")}
                value={it.ranges}
                onChange={(e) => updateRanges(idx, e.target.value)}
                style={{ marginTop: 4, fontSize: 12 }}
              />
            </div>
          );
        })}
      </div>
      {items.length > 0 && (
        <div className="merge-summary">
          <span>{t("合并顺序：{order}", { order: items.map((_, i) => i + 1).join(" → ") })}</span>
          <span>{t("合并后共 {n} 页", { n: totalSelected })}</span>
        </div>
      )}
      {items.length > 0 && hasError && (
        <p className="merge-hint">{t("请先修正标红的页码范围，或删除该文件")}</p>
      )}
      <div className="merge-output">
        <label>{t("输出")}</label>
        <span
          className="merge-output-path"
          title={outputPath ?? undefined}
          style={outputPath ? undefined : { color: "var(--fg-dim)" }}
        >
          {outputPath ?? t("未选择（结果仅打开到查看器）")}
        </span>
        <button onClick={pickOutput} disabled={busy}>
          {t("选择…")}
        </button>
        {outputPath && (
          <button title={t("清除")} onClick={() => setOutputPath(null)} disabled={busy}>
            ✕
          </button>
        )}
      </div>
      <div className="task-footer">
        <button className="btn-primary" onClick={doMerge} disabled={!canMerge}>
          {busy ? t("合并中…") : t("开始合并")}
        </button>
      </div>
    </div>
  );
}
