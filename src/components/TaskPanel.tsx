import { useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useApp } from "../state/store";
import type { TaskId } from "../state/store";
import {
  mergeDocuments,
  splitDocument,
  canUndo,
  addTextWatermark,
  addImageWatermark,
  addAnnotation,
  listAnnotations,
  deleteAnnotation,
  clearAnnotations,
  getSecurityStatus,
  exportPlainCopy,
  reloadPlain,
} from "../lib/ipc";
import type {
  SplitMode,
  WatermarkStyle,
  AnnotationInfo,
  AnnotationKind,
  SecurityStatus,
} from "../lib/ipc";

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
  edit: "为当前页添加 PDF 注释：高亮、下划线、删除线、便签、自由文本框、矩形标注。",
  security: "查看文档加密状态与权限矩阵；导出明文副本或在内存中去除加密后另存。",
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

const POSITIONS: { k: string; label: string }[] = [
  { k: "top-left", label: "◤" },
  { k: "top-center", label: "▲" },
  { k: "top-right", label: "◥" },
  { k: "middle-left", label: "◀" },
  { k: "center", label: "◉" },
  { k: "middle-right", label: "▶" },
  { k: "bottom-left", label: "◣" },
  { k: "bottom-center", label: "▼" },
  { k: "bottom-right", label: "◢" },
];

function WatermarkPanel() {
  const docId = useApp((s) => s.docId);
  const pageCount = useApp((s) => s.pageCount);
  const selectedPages = useApp((s) => s.selectedPages);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const setCanUndo = useApp((s) => s.setCanUndo);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const closeTask = useApp((s) => s.closeTask);

  const [kind, setKind] = useState<"text" | "image">("text");
  const [text, setText] = useState("仅供内部使用");
  const [fontSize, setFontSize] = useState(48);
  const [color, setColor] = useState("#ff0000");
  const [imagePath, setImagePath] = useState<string | null>(null);
  const [imageName, setImageName] = useState("");
  const [scale, setScale] = useState(30);
  const [opacity, setOpacity] = useState(30);
  const [rotation, setRotation] = useState(45);
  const [position, setPosition] = useState("center");
  const [tiled, setTiled] = useState(false);
  const [tileSpacing, setTileSpacing] = useState(120);
  const [onlySelected, setOnlySelected] = useState(false);
  const [busy, setBusy] = useState(false);

  const pickImage = async () => {
    const picked = await open({
      multiple: false,
      filters: [
        { name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "bmp"] },
      ],
    });
    if (typeof picked === "string") {
      setImagePath(picked);
      setImageName(picked.split(/[\\/]/).pop() || picked);
    }
  };

  const apply = async () => {
    if (docId === null) return;
    const pages =
      onlySelected && selectedPages.size > 0
        ? Array.from(selectedPages).sort((a, b) => a - b)
        : Array.from({ length: pageCount }, (_, i) => i);
    if (pages.length === 0) {
      pushToast("info", "没有可应用的页面");
      return;
    }
    const style: WatermarkStyle = { opacity, rotation, position, tiled, tileSpacing };
    setBusy(true);
    try {
      const info =
        kind === "text"
          ? await addTextWatermark(docId, pages, { text, fontSize, color, style })
          : await addImageWatermark(docId, pages, {
              imagePath: imagePath as string,
              scale: scale / 100,
              style,
            });
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId));
      pushToast("info", `已为 ${pages.length} 页添加水印`);
      closeTask();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const valid = kind === "text" ? text.trim().length > 0 : imagePath !== null;

  return (
    <div className="task-body">
      <div className="split-modes">
        <label className={`split-mode${kind === "text" ? " active" : ""}`}>
          <input
            type="radio"
            checked={kind === "text"}
            onChange={() => setKind("text")}
            style={{ display: "none" }}
          />
          文字水印
        </label>
        <label className={`split-mode${kind === "image" ? " active" : ""}`}>
          <input
            type="radio"
            checked={kind === "image"}
            onChange={() => setKind("image")}
            style={{ display: "none" }}
          />
          图片水印
        </label>
      </div>

      {kind === "text" ? (
        <>
          <div className="field">
            <label>水印文字</label>
            <input
              type="text"
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder="支持中文"
            />
          </div>
          <div className="form-row">
            <label>字号</label>
            <input
              type="number"
              min={8}
              max={200}
              value={fontSize}
              onChange={(e) =>
                setFontSize(Math.min(200, Math.max(8, parseInt(e.target.value) || 48)))
              }
              style={{ width: 64 }}
            />
            <label>颜色</label>
            <input
              type="color"
              value={color}
              onChange={(e) => setColor(e.target.value)}
              style={{ width: 40, padding: 0, height: 28 }}
            />
          </div>
        </>
      ) : (
        <>
          <div className="form-row">
            <button onClick={pickImage}>选择图片…</button>
            <span className="merge-name" title={imagePath ?? undefined}>
              {imageName || "未选择"}
            </span>
          </div>
          <div className="form-row">
            <label>宽度占页</label>
            <input
              type="number"
              min={5}
              max={100}
              value={scale}
              onChange={(e) =>
                setScale(Math.min(100, Math.max(5, parseInt(e.target.value) || 30)))
              }
              style={{ width: 64 }}
            />
            <label>%</label>
          </div>
        </>
      )}

      <div className="form-row">
        <label>透明度</label>
        <input
          type="range"
          min={5}
          max={100}
          value={opacity}
          onChange={(e) => setOpacity(parseInt(e.target.value))}
          style={{ flex: 1 }}
        />
        <span style={{ width: 34, textAlign: "right", color: "var(--fg-dim)" }}>
          {opacity}%
        </span>
      </div>
      <div className="form-row">
        <label>旋转角度</label>
        <input
          type="number"
          min={-180}
          max={180}
          value={rotation}
          onChange={(e) => setRotation(parseInt(e.target.value) || 0)}
          style={{ width: 64 }}
        />
        <label>度（顺时针）</label>
      </div>

      <div className="field">
        <label>位置</label>
        <div className="pos-grid">
          {POSITIONS.map((p) => (
            <button
              key={p.k}
              className={position === p.k ? "active" : ""}
              onClick={() => setPosition(p.k)}
            >
              {p.label}
            </button>
          ))}
        </div>
      </div>

      <div className="form-row">
        <label className="chk">
          <input
            type="checkbox"
            checked={tiled}
            onChange={(e) => setTiled(e.target.checked)}
          />
          平铺整页
        </label>
        {tiled && (
          <>
            <label>间距</label>
            <input
              type="number"
              min={20}
              max={600}
              value={tileSpacing}
              onChange={(e) =>
                setTileSpacing(Math.max(20, parseInt(e.target.value) || 120))
              }
              style={{ width: 64 }}
            />
            <label>pt</label>
          </>
        )}
      </div>

      {selectedPages.size > 0 && (
        <label className="chk">
          <input
            type="checkbox"
            checked={onlySelected}
            onChange={(e) => setOnlySelected(e.target.checked)}
          />
          仅应用到选中的 {selectedPages.size} 页
        </label>
      )}

      <div className="task-footer">
        <button
          className="btn-primary"
          onClick={apply}
          disabled={busy || docId === null || !valid}
        >
          {busy ? "添加中…" : "添加水印"}
        </button>
      </div>
    </div>
  );
}

const ANNOT_KINDS: { k: AnnotationKind; label: string; icon: string }[] = [
  { k: "Highlight", label: "高亮", icon: "🖍" },
  { k: "Underline", label: "下划线", icon: "U̲" },
  { k: "Strikeout", label: "删除线", icon: "S̶" },
  { k: "StickyNote", label: "便签", icon: "📝" },
  { k: "FreeText", label: "文字框", icon: "T" },
  { k: "Square", label: "矩形", icon: "▭" },
];

function EditPanel() {
  const docId = useApp((s) => s.docId);
  const currentPage = useApp((s) => s.currentPage);
  const pageCount = useApp((s) => s.pageCount);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const setCanUndo = useApp((s) => s.setCanUndo);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);

  const [kind, setKind] = useState<AnnotationKind>("Highlight");
  const [color, setColor] = useState("#ffeb3b");
  const [contents, setContents] = useState("");
  const [list, setList] = useState<AnnotationInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const reloadRef = useRef(0);

  const reload = async () => {
    if (docId === null) return;
    setLoading(true);
    const seq = ++reloadRef.current;
    try {
      const data = await listAnnotations(docId, currentPage);
      if (seq === reloadRef.current) setList(data);
    } catch (e) {
      if (seq === reloadRef.current) errorToast(e);
    } finally {
      if (seq === reloadRef.current) setLoading(false);
    }
  };

  useEffect(() => {
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [docId, currentPage]);

  const add = async () => {
    if (docId === null) return;
    setBusy(true);
    try {
      // 区域：默认放在页面上 1/3 处，宽 1/3、高 1/12
      const region = {
        left: 0.15,
        top: 0.2,
        width: 0.7,
        height: 0.07,
      };
      const info = await addAnnotation(docId, currentPage, {
        kind,
        region,
        contents,
        color,
        opacity: 60,
      });
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId));
      pushToast("info", `已添加 ${ANNOT_KINDS.find((x) => x.k === kind)?.label ?? ""}`);
      setContents("");
      await reload();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const remove = async (idx: number) => {
    if (docId === null) return;
    setBusy(true);
    try {
      const info = await deleteAnnotation(docId, currentPage, idx);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId));
      await reload();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const clearAll = async () => {
    if (docId === null) return;
    if (!confirm(`清空当前页（${list.length} 个注释）？`)) return;
    setBusy(true);
    try {
      const info = await clearAnnotations(docId, [currentPage]);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId));
      pushToast("info", "已清空当前页注释");
      await reload();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="task-body">
      <p className="placeholder">
        第 {currentPage + 1} / {pageCount || "?"} 页。点击下方按钮即可在页面顶部插入所选类型的注释。
      </p>
      <div className="annot-kinds">
        {ANNOT_KINDS.map((a) => (
          <button
            key={a.k}
            className={kind === a.k ? "active" : ""}
            onClick={() => setKind(a.k)}
            title={a.label}
          >
            <span style={{ fontSize: 16 }}>{a.icon}</span>
            <br />
            {a.label}
          </button>
        ))}
      </div>
      {(kind === "Highlight" ||
        kind === "Underline" ||
        kind === "Strikeout" ||
        kind === "FreeText" ||
        kind === "Square") && (
        <div className="form-row">
          <label>颜色</label>
          <input
            type="color"
            value={color}
            onChange={(e) => setColor(e.target.value)}
            style={{ width: 40, padding: 0, height: 28 }}
          />
        </div>
      )}
      <div className="field">
        <label>备注文本（可选）</label>
        <input
          type="text"
          value={contents}
          onChange={(e) => setContents(e.target.value)}
          placeholder="如：此处需补充说明"
        />
      </div>
      <div className="task-footer">
        <button
          className="btn-primary"
          onClick={add}
          disabled={busy || docId === null}
        >
          {busy ? "添加中…" : "添加到当前页"}
        </button>
      </div>

      <div className="annot-list-head">
        <span>本页注释（{loading ? "…" : list.length}）</span>
        {list.length > 0 && (
          <button onClick={clearAll} disabled={busy} style={{ color: "var(--danger)" }}>
            清空本页
          </button>
        )}
      </div>
      <div className="annot-list">
        {list.length === 0 && !loading && (
          <p className="placeholder">本页还没有注释</p>
        )}
        {list.map((a, i) => (
          <div key={i} className="annot-item">
            <span
              className="annot-swatch"
              style={{ background: a.color }}
            />
            <span className="annot-kind">{a.kind}</span>
            <span className="annot-contents" title={a.contents}>
              {a.contents || "（无文本）"}
            </span>
            <button
              onClick={() => remove(a.index)}
              disabled={busy}
              title="删除"
              style={{ color: "var(--danger)" }}
            >
              ✕
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

interface PermRow {
  key: keyof Omit<SecurityStatus, "handlerRevision">;
  label: string;
}

const PERM_ROWS: PermRow[] = [
  { key: "canPrintHighQuality", label: "高质量打印" },
  { key: "canPrintLowQuality", label: "低质量打印" },
  { key: "canModifyDocument", label: "修改文档内容" },
  { key: "canExtractTextAndGraphics", label: "抽取文本与图形" },
  { key: "canAddAnnotations", label: "添加或修改注释" },
  { key: "canFillFormFields", label: "填写表单字段" },
  { key: "canAssembleDocument", label: "组装文档（插页/旋转/删页等）" },
  { key: "canCreateNewFormFields", label: "创建新表单字段" },
];

function SecurityPanel() {
  const docId = useApp((s) => s.docId);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const setCanUndo = useApp((s) => s.setCanUndo);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);

  const [status, setStatus] = useState<SecurityStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);

  const reload = async () => {
    if (docId === null) return;
    setLoading(true);
    try {
      const s = await getSecurityStatus(docId);
      setStatus(s);
    } catch (e) {
      errorToast(e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    setStatus(null);
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [docId]);

  const isProtected = status?.handlerRevision !== "Unprotected" && status?.handlerRevision !== "Unknown";

  const onExportPlain = async () => {
    if (docId === null) return;
    const p = await save({
      title: "导出明文副本",
      defaultPath: "plain.pdf",
      filters: [{ name: "PDF 文档", extensions: ["pdf"] }],
    });
    if (!p) return;
    setBusy(true);
    try {
      await exportPlainCopy(docId, p);
      // 同时把内存里的 doc 也去掉加密
      const info = await reloadPlain(docId);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId));
      pushToast("info", `已导出明文副本：${p}`);
      await reload();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const onStripInMemory = async () => {
    if (docId === null) return;
    if (!confirm("去除当前文档的密码/加密？文件需另存到磁盘生效。")) return;
    setBusy(true);
    try {
      const info = await reloadPlain(docId);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId));
      pushToast("info", "已去除内存中的加密，请立即 Ctrl+S 另存");
      await reload();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="task-body">
      <p className="placeholder">
        pdfium-render 仅支持读取文档的加密状态与权限矩阵，不支持修改加密设置。
        加密文档可以导出为明文副本，或在内存中去加密后另存为。
      </p>

      {loading && <p className="placeholder">加载中…</p>}
      {status && (
        <>
          <div className="security-status">
            <span className="security-status-label">加密状态</span>
            <span
              className={`security-badge ${isProtected ? "protected" : "unprotected"}`}
            >
              {isProtected ? `已加密（${status.handlerRevision}）` : "未加密"}
            </span>
          </div>

          <div className="security-perms">
            {PERM_ROWS.map((row) => {
              const ok = status[row.key];
              return (
                <div key={row.key} className="perm-row">
                  <span className={`perm-dot ${ok ? "ok" : "deny"}`} />
                  <span className="perm-label">{row.label}</span>
                  <span className="perm-result">{ok ? "允许" : "禁止"}</span>
                </div>
              );
            })}
          </div>

          <div className="task-footer">
            <button
              className="btn-primary"
              onClick={onExportPlain}
              disabled={busy || docId === null}
            >
              {busy ? "导出中…" : isProtected ? "导出明文副本" : "导出当前文档"}
            </button>
            {isProtected && (
              <button
                onClick={onStripInMemory}
                disabled={busy}
                style={{ marginLeft: 6 }}
              >
                在内存中去除加密
              </button>
            )}
          </div>
        </>
      )}
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
        {task === "watermark" && <WatermarkPanel />}
        {task === "edit" && <EditPanel />}
        {task === "security" && <SecurityPanel />}
        {task !== "merge" && task !== "split" && task !== "watermark" && task !== "edit" && task !== "security" && (
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
