import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useApp } from "../state/store";
import type { TaskId } from "../state/store";
import { mergeDocuments, splitDocument, canUndo } from "../lib/ipc";
import type { SplitMode } from "../lib/ipc";

const TITLES: Record<Exclude<TaskId, null>, string> = {
  merge: "合并文档",
  split: "拆分文档",
  watermark: "水印",
  edit: "内容编辑",
  security: "文档安全",
  export: "导出图片",
};

const DESC: Record<Exclude<TaskId, null>, string> = {
  merge: "将多个 PDF 按顺序合并为一个文档，可对每个文件选择页码范围。",
  split: "按固定页数、自定义范围或书签层级，将文档拆分为多个文件。",
  watermark: "为页面添加文字或图片水印，支持位置、透明度与平铺。",
  edit: "双击页面文字进入编辑；新增文本框；选中图片可移动、缩放、删除。",
  security: "为文档设置打开密码与权限密码，或移除已有密码。",
  export: "将页面导出为 PNG / JPG 图片。",
};

interface MergeItem {
  path: string;
  name: string;
  ranges: string;
}

function MergePanel() {
  const [items, setItems] = useState<MergeItem[]>([]);
  const [outputPath, setOutputPath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const setDoc = useApp((s) => s.setDoc);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const closeTask = useApp((s) => s.closeTask);

  const addFiles = async () => {
    const picked = await open({
      multiple: true,
      filters: [{ name: "PDF 文档", extensions: ["pdf"] }],
    });
    if (Array.isArray(picked)) {
      setItems((prev) => [
        ...prev,
        ...picked.map((p) => ({
          path: p,
          name: p.split(/[\\/]/).pop() || p,
          ranges: "",
        })),
      ]);
    }
  };

  const pickOutput = async () => {
    const p = await save({
      title: "选择合并结果保存位置",
      defaultPath: "merged.pdf",
      filters: [{ name: "PDF 文档", extensions: ["pdf"] }],
    });
    if (typeof p === "string") setOutputPath(p);
  };

  const move = (idx: number, dir: -1 | 1) => {
    setItems((prev) => {
      const next = [...prev];
      const j = idx + dir;
      if (j < 0 || j >= next.length) return prev;
      [next[idx], next[j]] = [next[j], next[idx]];
      return next;
    });
  };

  const remove = (idx: number) => {
    setItems((prev) => prev.filter((_, i) => i !== idx));
  };

  const updateRanges = (idx: number, ranges: string) => {
    setItems((prev) => prev.map((it, i) => (i === idx ? { ...it, ranges } : it)));
  };

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
          ? `合并完成并已保存：${outputPath}（共 ${info.pageCount} 页）`
          : `合并完成，共 ${info.pageCount} 页`,
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
        ➕ 添加文件
      </button>
      <div className="merge-list">
        {items.length === 0 && (
          <p className="placeholder">点击上方按钮添加要合并的 PDF 文件</p>
        )}
        {items.map((it, idx) => (
          <div key={idx} className="merge-item">
            <div className="merge-row">
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
                <button
                  onClick={() => remove(idx)}
                  style={{ color: "var(--danger)" }}
                >
                  ✕
                </button>
              </div>
            </div>
            <input
              type="text"
              placeholder="页码范围，如 1,3,5-7（留空为全部页）"
              value={it.ranges}
              onChange={(e) => updateRanges(idx, e.target.value)}
              style={{ marginTop: 4, fontSize: 12 }}
            />
          </div>
        ))}
      </div>
      <div className="merge-output">
        <label>输出</label>
        <span
          className="merge-output-path"
          title={outputPath ?? undefined}
          style={outputPath ? undefined : { color: "var(--fg-dim)" }}
        >
          {outputPath ?? "未选择（结果仅打开到查看器）"}
        </span>
        <button onClick={pickOutput} disabled={busy}>
          选择…
        </button>
        {outputPath && (
          <button title="清除" onClick={() => setOutputPath(null)} disabled={busy}>
            ✕
          </button>
        )}
      </div>
      <div className="task-footer">
        <button
          className="btn-primary"
          onClick={doMerge}
          disabled={busy || items.length === 0}
        >
          {busy ? "合并中…" : "开始合并"}
        </button>
      </div>
    </div>
  );
}

function SplitPanel() {
  const docId = useApp((s) => s.docId);
  const pageCount = useApp((s) => s.pageCount);
  const selectedPages = useApp((s) => s.selectedPages);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);

  const [mode, setMode] = useState<"every_n" | "ranges" | "by_bookmark" | "selected">(
    "every_n",
  );
  const [everyN, setEveryN] = useState(1);
  const [ranges, setRanges] = useState("");
  const [bookmarkLevel, setBookmarkLevel] = useState(1);
  const [busy, setBusy] = useState(false);

  const doSplit = async () => {
    if (docId === null) return;
    let modePayload: SplitMode;
    switch (mode) {
      case "every_n":
        modePayload = { mode: "every_n", payload: { n: everyN } };
        break;
      case "ranges":
        if (!ranges.trim()) {
          pushToast("info", "请输入页码范围");
          return;
        }
        modePayload = { mode: "ranges", payload: { ranges } };
        break;
      case "by_bookmark":
        modePayload = { mode: "by_bookmark", payload: { level: bookmarkLevel } };
        break;
      case "selected":
        if (selectedPages.size === 0) {
          pushToast("info", "请先在缩略图中选择页面");
          return;
        }
        modePayload = { mode: "selected", payload: { pages: Array.from(selectedPages) } };
        break;
    }
    const outDir = await open({
      title: "选择输出目录",
      directory: true,
    });
    if (typeof outDir !== "string") return;
    setBusy(true);
    try {
      const outputs = await splitDocument(docId, modePayload, outDir);
      pushToast("info", `拆分完成，共生成 ${outputs.length} 个文件`);
      canUndo(docId).then(() => {}).catch(() => {});
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="task-body">
      <div className="split-modes">
        {[
          { k: "every_n", label: "每 N 页一份" },
          { k: "ranges", label: "自定义范围" },
          { k: "by_bookmark", label: "按书签顶层" },
          { k: "selected", label: "提取选中页" },
        ].map((m) => (
          <label
            key={m.k}
            className={`split-mode${mode === m.k ? " active" : ""}`}
          >
            <input
              type="radio"
              checked={mode === m.k}
              onChange={() => setMode(m.k as typeof mode)}
              style={{ display: "none" }}
            />
            {m.label}
          </label>
        ))}
      </div>
      {mode === "every_n" && (
        <div className="form-row">
          <label>每</label>
          <input
            type="number"
            min={1}
            max={pageCount || 1}
            value={everyN}
            onChange={(e) => setEveryN(Math.max(1, parseInt(e.target.value) || 1))}
            style={{ width: 60 }}
          />
          <label>页拆分为一份</label>
        </div>
      )}
      {mode === "ranges" && (
        <div className="form-row">
          <input
            type="text"
            placeholder="如 1-3,5,7-9"
            value={ranges}
            onChange={(e) => setRanges(e.target.value)}
            style={{ flex: 1 }}
          />
        </div>
      )}
      {mode === "by_bookmark" && (
        <div className="form-row">
          <label>按第</label>
          <input
            type="number"
            min={1}
            max={10}
            value={bookmarkLevel}
            onChange={(e) =>
              setBookmarkLevel(Math.max(1, parseInt(e.target.value) || 1))
            }
            style={{ width: 60 }}
          />
          <label>级书签拆分</label>
        </div>
      )}
      {mode === "selected" && (
        <p className="placeholder">
          已选择 {selectedPages.size} 页，每页将单独保存为一个文件。
        </p>
      )}
      <div className="task-footer">
        <button
          className="btn-primary"
          onClick={doSplit}
          disabled={busy || docId === null}
        >
          {busy ? "拆分中…" : "开始拆分"}
        </button>
      </div>
    </div>
  );
}

export default function TaskPanel() {
  const task = useApp((s) => s.task);
  const closeTask = useApp((s) => s.closeTask);

  if (task === null) return null;

  return (
    <aside className="task">
      <h3>
        <span>{TITLES[task]}</span>
        <button title="关闭面板" onClick={closeTask}>
          ✕
        </button>
      </h3>
      <div className="body">
        {task === "merge" && <MergePanel />}
        {task === "split" && <SplitPanel />}
        {task !== "merge" && task !== "split" && (
          <>
            <p className="placeholder">{DESC[task]}</p>
            <p className="placeholder" style={{ marginTop: 12 }}>
              该功能将在后续里程碑（M4–M6）中交付。
            </p>
          </>
        )}
      </div>
    </aside>
  );
}
