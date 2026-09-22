import { useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useApp } from "../state/store";
import type { TaskId } from "../state/store";
import {
  mergeDocuments,
  splitDocument,
  canUndo,
  canRedo,
  addTextWatermark,
  addImageWatermark,
  removeObjectsInRect,
  detectWatermarkCandidates,
  applyWatermarkRemoval,
  rewriteText,
  addTextBox,
  replaceImage,
  deleteImageObject,
  isScannedPage,
  clearPageText,
  addAnnotation,
  listAnnotations,
  deleteAnnotation,
  clearAnnotations,
  getSecurityStatus,
  exportPlainCopy,
  reloadPlain,
  exportPagesToImages,
  imagesToPdf,
  detectOffice,
  convertOfficeToPdf,
  detectEbookTools,
  convertEbookToPdf,
} from "../lib/ipc";
import type {
  SplitMode,
  WatermarkStyle,
  AnnotationInfo,
  AnnotationKind,
  SecurityStatus,
  OfficeProbe,
  EbookToolProbe,
  DetectResult,
  ObjectFingerprint,
  Rect as RemoveRect,
} from "../lib/ipc";
import { useT } from "../i18n";

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
  const t = useT();

  const addFiles = async () => {
    const picked = await open({
      multiple: true,
      filters: [{ name: t("PDF 文档"), extensions: ["pdf"] }],
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
      title: t("选择合并结果保存位置"),
      defaultPath: "merged.pdf",
      filters: [{ name: t("PDF 文档"), extensions: ["pdf"] }],
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
          ? t("合并完成并已保存：{path}（共 {n} 页）", { path: outputPath, n: info.pageCount })
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
      <div className="merge-list">
        {items.length === 0 && (
          <p className="placeholder">{t("点击上方按钮添加要合并的 PDF 文件")}</p>
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
              placeholder={t("页码范围，如 1,3,5-7（留空为全部页）")}
              value={it.ranges}
              onChange={(e) => updateRanges(idx, e.target.value)}
              style={{ marginTop: 4, fontSize: 12 }}
            />
          </div>
        ))}
      </div>
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
        <button
          className="btn-primary"
          onClick={doMerge}
          disabled={busy || items.length === 0}
        >
          {busy ? t("合并中…") : t("开始合并")}
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
  const t = useT();

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
          pushToast("info", t("请输入页码范围"));
          return;
        }
        modePayload = { mode: "ranges", payload: { ranges } };
        break;
      case "by_bookmark":
        modePayload = { mode: "by_bookmark", payload: { level: bookmarkLevel } };
        break;
      case "selected":
        if (selectedPages.size === 0) {
          pushToast("info", t("请先在缩略图中选择页面"));
          return;
        }
        modePayload = { mode: "selected", payload: { pages: Array.from(selectedPages) } };
        break;
    }
    const outDir = await open({
      title: t("选择输出目录"),
      directory: true,
    });
    if (typeof outDir !== "string") return;
    setBusy(true);
    try {
      const outputs = await splitDocument(docId, modePayload, outDir);
      pushToast("info", t("拆分完成，共生成 {n} 个文件", { n: outputs.length }));
      canUndo(docId).then(() => {}).catch(() => {});
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const splitModes = [
    { k: "every_n", label: t("每 N 页一份") },
    { k: "ranges", label: t("自定义范围") },
    { k: "by_bookmark", label: t("按书签顶层") },
    { k: "selected", label: t("提取选中页") },
  ];

  return (
    <div className="task-body">
      <div className="split-modes">
        {splitModes.map((m) => (
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
          <label>{t("每")}</label>
          <input
            type="number"
            min={1}
            max={pageCount || 1}
            value={everyN}
            onChange={(e) => setEveryN(Math.max(1, parseInt(e.target.value) || 1))}
            style={{ width: 60 }}
          />
          <label>{t("页拆分为一份")}</label>
        </div>
      )}
      {mode === "ranges" && (
        <div className="form-row">
          <input
            type="text"
            placeholder={t("如 1-3,5,7-9")}
            value={ranges}
            onChange={(e) => setRanges(e.target.value)}
            style={{ flex: 1 }}
          />
        </div>
      )}
      {mode === "by_bookmark" && (
        <div className="form-row">
          <label>{t("按第")}</label>
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
          <label>{t("级书签拆分")}</label>
        </div>
      )}
      {mode === "selected" && (
        <p className="placeholder">
          {t("已选择 {n} 页，每页将单独保存为一个文件。", { n: selectedPages.size })}
        </p>
      )}
      <div className="task-footer">
        <button
          className="btn-primary"
          onClick={doSplit}
          disabled={busy || docId === null}
        >
          {busy ? t("拆分中…") : t("开始拆分")}
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
  const [mode, setMode] = useState<"add" | "remove">("add");
  return (
    <>
      <div className="split-modes" style={{ marginBottom: 12 }}>
        <label className={`split-mode${mode === "add" ? " active" : ""}`}>
          <input
            type="radio"
            checked={mode === "add"}
            onChange={() => setMode("add")}
            style={{ display: "none" }}
          />
          <WatermarkAddLabel />
        </label>
        <label className={`split-mode${mode === "remove" ? " active" : ""}`}>
          <input
            type="radio"
            checked={mode === "remove"}
            onChange={() => setMode("remove")}
            style={{ display: "none" }}
          />
          <WatermarkRemoveLabel />
        </label>
      </div>
      {mode === "add" ? <WatermarkAddPanel /> : <WatermarkRemovePanel />}
    </>
  );
}

function WatermarkAddLabel() {
  const t = useT();
  return <>{t("添加水印")}</>;
}

function WatermarkRemoveLabel() {
  const t = useT();
  return <>{t("去除水印")}</>;
}

function WatermarkAddPanel() {
  const docId = useApp((s) => s.docId);
  const pageCount = useApp((s) => s.pageCount);
  const selectedPages = useApp((s) => s.selectedPages);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const setCanUndo = useApp((s) => s.setCanUndo);
  const setCanRedo = useApp((s) => s.setCanRedo);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const closeTask = useApp((s) => s.closeTask);
  const t = useT();

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
        { name: t("图片"), extensions: ["png", "jpg", "jpeg", "webp", "bmp"] },
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
      pushToast("info", t("没有可应用的页面"));
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
      setCanUndo(await canUndo(docId)); setCanRedo(await canRedo(docId));
      pushToast("info", t("已为 {n} 页添加水印", { n: pages.length }));
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
          {t("文字水印")}
        </label>
        <label className={`split-mode${kind === "image" ? " active" : ""}`}>
          <input
            type="radio"
            checked={kind === "image"}
            onChange={() => setKind("image")}
            style={{ display: "none" }}
          />
          {t("图片水印")}
        </label>
      </div>

      {kind === "text" ? (
        <>
          <div className="field">
            <label>{t("水印文字")}</label>
            <input
              type="text"
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder={t("支持中文")}
            />
          </div>
          <div className="form-row">
            <label>{t("字号")}</label>
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
            <label>{t("颜色")}</label>
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
            <button onClick={pickImage}>{t("选择图片…")}</button>
            <span className="merge-name" title={imagePath ?? undefined}>
              {imageName || t("未选择")}
            </span>
          </div>
          <div className="form-row">
            <label>{t("宽度占页")}</label>
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
        <label>{t("透明度")}</label>
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
        <label>{t("旋转角度")}</label>
        <input
          type="number"
          min={-180}
          max={180}
          value={rotation}
          onChange={(e) => setRotation(parseInt(e.target.value) || 0)}
          style={{ width: 64 }}
        />
        <label>{t("度（顺时针）")}</label>
      </div>

      <div className="field">
        <label>{t("位置")}</label>
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
          {t("平铺整页")}
        </label>
        {tiled && (
          <>
            <label>{t("间距")}</label>
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
          {t("仅应用到选中的 {n} 页", { n: selectedPages.size })}
        </label>
      )}

      <div className="task-footer">
        <button
          className="btn-primary"
          onClick={apply}
          disabled={busy || docId === null || !valid}
        >
          {busy ? t("添加中…") : t("添加水印")}
        </button>
      </div>
    </div>
  );
}

function WatermarkRemovePanel() {
  const docId = useApp((s) => s.docId);
  const pageCount = useApp((s) => s.pageCount);
  const currentPage = useApp((s) => s.currentPage);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const setCanUndo = useApp((s) => s.setCanUndo);
  const setCanRedo = useApp((s) => s.setCanRedo);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const closeTask = useApp((s) => s.closeTask);
  const t = useT();

  type SubMode = "manual" | "auto";
  const [subMode, setSubMode] = useState<SubMode>("manual");
  const [busy, setBusy] = useState(false);
  // manual
  const [rect, setRect] = useState<RemoveRect>({
    left: 100,
    bottom: 100,
    right: 300,
    top: 200,
  });
  const [onlyCurrent, setOnlyCurrent] = useState(true);
  // auto
  const [samplePages, setSamplePages] = useState(10);
  const [threshold, setThreshold] = useState(0.6);
  const [detectResult, setDetectResult] = useState<DetectResult | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [selectedFp, setSelectedFp] = useState<Set<number>>(new Set());

  const requireDoc = (): number | null => {
    if (docId === null) {
      pushToast("info", t("请打开文档后再操作"));
      return null;
    }
    return docId;
  };

  const validRect =
    rect.right > rect.left && rect.top > rect.bottom;

  const applyManual = async () => {
    const id = requireDoc();
    if (!id || !validRect) {
      pushToast("info", t("请检查矩形坐标"));
      return;
    }
    const pages = onlyCurrent ? [currentPage] : Array.from({ length: pageCount }, (_, i) => i);
    setBusy(true);
    try {
      const res = await removeObjectsInRect(id, pages, rect);
      updatePages(res.info);
      markDirty(true);
      setCanUndo(await canUndo(id));
      setCanRedo(await canRedo(id));
      pushToast("info", t("已删除 {n} 个对象", { n: res.removedCount }));
      closeTask();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const runDetect = async () => {
    const id = requireDoc();
    if (!id) return;
    setDetecting(true);
    try {
      const res = await detectWatermarkCandidates(id, samplePages, threshold);
      setDetectResult(res);
      setSelectedFp(new Set(res.candidates.map((c) => c.objectIndex)));
    } catch (e) {
      errorToast(e);
    } finally {
      setDetecting(false);
    }
  };

  const applyAuto = async () => {
    const id = requireDoc();
    if (!id || !detectResult || selectedFp.size === 0) {
      pushToast("info", t("未检测到候选水印。请先点击「开始检测」。"));
      return;
    }
    const indices = Array.from(selectedFp).sort((a, b) => b - a);
    setBusy(true);
    try {
      const res = await applyWatermarkRemoval(id, indices);
      updatePages(res.info);
      markDirty(true);
      setCanUndo(await canUndo(id));
      setCanRedo(await canRedo(id));
      pushToast("info", t("已删除 {n} 个对象", { n: res.removedCount }));
      closeTask();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const toggleFp = (idx: number) => {
    setSelectedFp((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  };

  return (
    <div className="task-body">
      <p className="placeholder">
        {t("在当前页用鼠标框选区域，或自动检测重复水印对象")}
      </p>
      <div className="split-modes">
        <label className={`split-mode${subMode === "manual" ? " active" : ""}`}>
          <input
            type="radio"
            checked={subMode === "manual"}
            onChange={() => setSubMode("manual")}
            style={{ display: "none" }}
          />
          {t("手动框选")}
        </label>
        <label className={`split-mode${subMode === "auto" ? " active" : ""}`}>
          <input
            type="radio"
            checked={subMode === "auto"}
            onChange={() => setSubMode("auto")}
            style={{ display: "none" }}
          />
          {t("自动检测")}
        </label>
      </div>

      {subMode === "manual" ? (
        <>
          <div className="field">
            <label>{t("矩形坐标（PDF 点，左下原点）")}</label>
          </div>
          <div className="form-row">
            <label>{t("左")}</label>
            <input
              type="number"
              value={rect.left}
              onChange={(e) => setRect({ ...rect, left: parseFloat(e.target.value) || 0 })}
              style={{ width: 70 }}
            />
            <label>{t("下")}</label>
            <input
              type="number"
              value={rect.bottom}
              onChange={(e) => setRect({ ...rect, bottom: parseFloat(e.target.value) || 0 })}
              style={{ width: 70 }}
            />
          </div>
          <div className="form-row">
            <label>{t("右")}</label>
            <input
              type="number"
              value={rect.right}
              onChange={(e) => setRect({ ...rect, right: parseFloat(e.target.value) || 0 })}
              style={{ width: 70 }}
            />
            <label>{t("上")}</label>
            <input
              type="number"
              value={rect.top}
              onChange={(e) => setRect({ ...rect, top: parseFloat(e.target.value) || 0 })}
              style={{ width: 70 }}
            />
          </div>
          <label className="chk">
            <input
              type="checkbox"
              checked={onlyCurrent}
              onChange={(e) => setOnlyCurrent(e.target.checked)}
            />
            {t("应用到当前页")}
          </label>
          <p className="placeholder" style={{ marginTop: 8, fontSize: 11 }}>
            {t("建议：先在画布上选区 → 自动获取矩形 → 确认。")}
          </p>
          <div className="task-footer">
            <button
              className="btn-primary"
              onClick={applyManual}
              disabled={busy || !validRect || docId === null}
            >
              {busy ? t("删除中…") : t("去除水印")}
            </button>
            <button
              onClick={() => setRect({ left: 100, bottom: 100, right: 300, top: 200 })}
              disabled={busy}
              style={{ marginLeft: 6 }}
            >
              {t("清除")}
            </button>
          </div>
        </>
      ) : (
        <>
          <div className="form-row">
            <label>{t("采样页数")}</label>
            <input
              type="number"
              min={2}
              max={50}
              value={samplePages}
              onChange={(e) =>
                setSamplePages(Math.min(50, Math.max(2, parseInt(e.target.value) || 10)))
              }
              style={{ width: 60 }}
            />
            <label>{t("出现阈值")}</label>
            <input
              type="number"
              min={0.3}
              max={1.0}
              step={0.05}
              value={threshold}
              onChange={(e) =>
                setThreshold(Math.min(1.0, Math.max(0.3, parseFloat(e.target.value) || 0.6)))
              }
              style={{ width: 60 }}
            />
          </div>
          <div className="task-footer">
            <button
              className="btn-primary"
              onClick={runDetect}
              disabled={detecting || busy || docId === null}
            >
              {detecting ? t("检测中…") : detectResult ? t("重新检测") : t("开始检测")}
            </button>
          </div>

          {detectResult && (
            <>
              <p className="placeholder">
                {t("候选水印（出现在多页）")}（{detectResult.candidates.length}）
              </p>
              {detectResult.candidates.length === 0 ? (
                <p className="placeholder">{t("无候选水印")}</p>
              ) : (
                <div className="wm-candidate-list">
                  {detectResult.candidates.map((fp) => (
                    <WatermarkCandidateRow
                      key={fp.objectIndex}
                      fp={fp}
                      totalSampled={detectResult.sampledPages}
                      checked={selectedFp.has(fp.objectIndex)}
                      onToggle={() => toggleFp(fp.objectIndex)}
                    />
                  ))}
                </div>
              )}
              <div className="task-footer">
                <button
                  className="btn-primary"
                  onClick={applyAuto}
                  disabled={busy || selectedFp.size === 0}
                >
                  {busy ? t("删除中…") : t("应用去除")}
                </button>
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}

function WatermarkCandidateRow({
  fp,
  totalSampled,
  checked,
  onToggle,
}: {
  fp: ObjectFingerprint;
  totalSampled: number;
  checked: boolean;
  onToggle: () => void;
}) {
  const t = useT();
  return (
    <label className="wm-candidate-row">
      <input type="checkbox" checked={checked} onChange={onToggle} />
      <span className="wm-candidate-kind">
        {fp.kind === "text" ? "T" : fp.kind === "image" ? "🖼" : "▭"}
      </span>
      <span className="wm-candidate-info">
        {t("类型")}: {fp.kind} · {t("位置")}: ({fp.left.toFixed(0)}, {fp.bottom.toFixed(0)}) – ({fp.right.toFixed(0)}, {fp.top.toFixed(0)}) · {t("出现")}: {fp.occurrence}/{totalSampled}
      </span>
    </label>
  );
}

const ANNOT_KINDS: { k: AnnotationKind; labelKey: string; icon: string }[] = [
  { k: "Highlight", labelKey: "高亮", icon: "🖍" },
  { k: "Underline", labelKey: "下划线", icon: "U̲" },
  { k: "Strikeout", labelKey: "删除线", icon: "S̶" },
  { k: "StickyNote", labelKey: "便签", icon: "📝" },
  { k: "FreeText", labelKey: "文字框", icon: "T" },
  { k: "Square", labelKey: "矩形", icon: "▭" },
];

function EditPanel() {
  const [mode, setMode] = useState<"annot" | "deep">("annot");
  return (
    <>
      <div className="split-modes" style={{ marginBottom: 12 }}>
        <label className={`split-mode${mode === "annot" ? " active" : ""}`}>
          <input
            type="radio"
            checked={mode === "annot"}
            onChange={() => setMode("annot")}
            style={{ display: "none" }}
          />
          <EditAnnotLabel />
        </label>
        <label className={`split-mode${mode === "deep" ? " active" : ""}`}>
          <input
            type="radio"
            checked={mode === "deep"}
            onChange={() => setMode("deep")}
            style={{ display: "none" }}
          />
          <EditDeepLabel />
        </label>
      </div>
      {mode === "annot" ? <AnnotationEditor /> : <DeepEditor />}
    </>
  );
}

function EditAnnotLabel() {
  const t = useT();
  return <>{t("注释")}</>;
}

function EditDeepLabel() {
  const t = useT();
  return <>{t("深度编辑")}</>;
}

function AnnotationEditor() {
  const docId = useApp((s) => s.docId);
  const currentPage = useApp((s) => s.currentPage);
  const pageCount = useApp((s) => s.pageCount);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const setCanUndo = useApp((s) => s.setCanUndo);
  const setCanRedo = useApp((s) => s.setCanRedo);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const t = useT();

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
      setCanUndo(await canUndo(docId)); setCanRedo(await canRedo(docId));
      const kindLabel = ANNOT_KINDS.find((x) => x.k === kind)?.labelKey ?? "";
      pushToast("info", t("已添加 {kind}", { kind: t(kindLabel) }));
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
      setCanUndo(await canUndo(docId)); setCanRedo(await canRedo(docId));
      await reload();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const clearAll = async () => {
    if (docId === null) return;
    if (!confirm(t("清空当前页（{n} 个注释）？", { n: list.length }))) return;
    setBusy(true);
    try {
      const info = await clearAnnotations(docId, [currentPage]);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId)); setCanRedo(await canRedo(docId));
      pushToast("info", t("已清空当前页注释"));
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
        {t("第 {n} / {total} 页。点击下方按钮即可在页面顶部插入所选类型的注释。", {
          n: currentPage + 1,
          total: pageCount || "?",
        })}
      </p>
      <div className="annot-kinds">
        {ANNOT_KINDS.map((a) => (
          <button
            key={a.k}
            className={kind === a.k ? "active" : ""}
            onClick={() => setKind(a.k)}
            title={t(a.labelKey)}
          >
            <span style={{ fontSize: 16 }}>{a.icon}</span>
            <br />
            {t(a.labelKey)}
          </button>
        ))}
      </div>
      {(kind === "Highlight" ||
        kind === "Underline" ||
        kind === "Strikeout" ||
        kind === "FreeText" ||
        kind === "Square") && (
        <div className="form-row">
          <label>{t("颜色")}</label>
          <input
            type="color"
            value={color}
            onChange={(e) => setColor(e.target.value)}
            style={{ width: 40, padding: 0, height: 28 }}
          />
        </div>
      )}
      <div className="field">
        <label>{t("备注文本（可选）")}</label>
        <input
          type="text"
          value={contents}
          onChange={(e) => setContents(e.target.value)}
          placeholder={t("如：此处需补充说明")}
        />
      </div>
      <div className="task-footer">
        <button
          className="btn-primary"
          onClick={add}
          disabled={busy || docId === null}
        >
          {busy ? t("添加中…") : t("添加到当前页")}
        </button>
      </div>

      <div className="annot-list-head">
        <span>{t("本页注释（{n}）", { n: loading ? "…" : list.length })}</span>
        {list.length > 0 && (
          <button onClick={clearAll} disabled={busy} style={{ color: "var(--danger)" }}>
            {t("清空本页")}
          </button>
        )}
      </div>
      <div className="annot-list">
        {list.length === 0 && !loading && (
          <p className="placeholder">{t("本页还没有注释")}</p>
        )}
        {list.map((a, i) => (
          <div key={i} className="annot-item">
            <span
              className="annot-swatch"
              style={{ background: a.color }}
            />
            <span className="annot-kind">{a.kind}</span>
            <span className="annot-contents" title={a.contents}>
              {a.contents || t("（无文本）")}
            </span>
            <button
              onClick={() => remove(a.index)}
              disabled={busy}
              title={t("删除")}
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

function DeepEditor() {
  const docId = useApp((s) => s.docId);
  const currentPage = useApp((s) => s.currentPage);
  const pageCount = useApp((s) => s.pageCount);
  const pages = useApp((s) => s.pages);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const setCanUndo = useApp((s) => s.setCanUndo);
  const setCanRedo = useApp((s) => s.setCanRedo);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const closeTask = useApp((s) => s.closeTask);
  const setSelectingFor = useApp((s) => s.setSelectingFor);
  const completedSelection = useApp((s) => s.completedSelection);
  const setCompletedSelection = useApp((s) => s.setCompletedSelection);
  const t = useT();

  type Mode = "rewrite" | "addtext" | "image" | "scandetect";
  const [mode, setMode] = useState<Mode>("rewrite");
  const [busy, setBusy] = useState(false);

  // rewrite
  const [region, setRegion] = useState({
    left: 100,
    bottom: 100,
    right: 300,
    top: 200,
  });
  const [newText] = useState("");
  const [rwFontSize] = useState(24);
  const [rwColor] = useState("#000000");

  // addtext
  const [textInput, setTextInput] = useState("");
  const [tbFontSize, setTbFontSize] = useState(24);
  const [tbColor, setTbColor] = useState("#000000");
  const [tbX, setTbX] = useState(100);
  const [tbY, setTbY] = useState(100);

  // image
  const [imgPath, setImgPath] = useState<string | null>(null);
  const [imgName, setImgName] = useState("");
  const [objIndex, setObjIndex] = useState(0);

  // scandetect
  const [scanState, setScanState] = useState<"unknown" | "scanned" | "not">(
    "unknown",
  );
  const [detecting, setDetecting] = useState(false);

  // 选区模式：仅"文字重写"允许使用画布选区
  useEffect(() => {
    if (mode === "rewrite") {
      setSelectingFor("rewrite");
    } else {
      setSelectingFor(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode]);

  // 文字重写 modal 状态
  const [rwModal, setRwModal] = useState<{
    region: { left: number; bottom: number; right: number; top: number };
    pageIndex: number;
  } | null>(null);

  // 选区到 PDF 点的坐标转换：CSS 像素 / scale = PDF 点
  // Y 轴翻转：CSS 顶 → PDF 底（pdf_top = pageH_pdf - (css_top / scale)）
  useEffect(() => {
    if (!completedSelection || docId === null) return;
    if (completedSelection.pageIndex !== currentPage && pages.length > 0) {
      // 选中发生在别的页，自动切换
      // 这里交给调用方处理；不强制切页，避免意外滚动
    }
    const sel = completedSelection;
    const pageInfo = pages[sel.pageIndex];
    if (!pageInfo) return;
    const s = sel.scale;
    if (s <= 0) return;
    const pageH_pt = pageInfo.height;
    const left_pt = sel.rect.left / s;
    const top_css = sel.rect.top;
    const right_pt = sel.rect.right / s;
    const bottom_css = sel.rect.bottom;
    // 转 PDF 点（左下原点）
    const leftPdf = left_pt;
    const rightPdf = right_pt;
    const topPdf = pageH_pt - top_css / s;
    const bottomPdf = pageH_pt - bottom_css / s;
    setRwModal({
      region: { left: leftPdf, bottom: bottomPdf, right: rightPdf, top: topPdf },
      pageIndex: sel.pageIndex,
    });
    setCompletedSelection(null); // 用完清空
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [completedSelection]);

  const requireDoc = (): number | null => {
    if (docId === null) {
      pushToast("info", t("请打开文档后再操作"));
      return null;
    }
    return docId;
  };

  const onRewrite = async (opts: {
    region: { left: number; bottom: number; right: number; top: number };
    pageIndex: number;
    newText: string;
    fontSize: number;
    color: string;
  }) => {
    const id = requireDoc();
    if (!id) return;
    if (!opts.newText.trim()) {
      pushToast("info", t("文字不能为空"));
      return;
    }
    setBusy(true);
    try {
      const info = await rewriteText(id, opts.pageIndex, opts.region, {
        newText: opts.newText.trim(),
        fontSize: opts.fontSize,
        color: opts.color,
      });
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(id));
      setCanRedo(await canRedo(id));
      pushToast(
        "info",
        t("已重写第 {n} 页文字", { n: opts.pageIndex + 1 }),
      );
      setRwModal(null);
      closeTask();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const onAddText = async () => {
    const id = requireDoc();
    if (!id) return;
    if (!textInput.trim()) {
      pushToast("info", t("文字不能为空"));
      return;
    }
    setBusy(true);
    try {
      const info = await addTextBox(id, currentPage, {
        text: textInput.trim(),
        fontSize: tbFontSize,
        color: tbColor,
        x: tbX,
        y: tbY,
      });
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(id));
      setCanRedo(await canRedo(id));
      pushToast("info", t("已添加文本"));
      closeTask();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const pickImage = async () => {
    const picked = await open({
      multiple: false,
      filters: [
        {
          name: t("图片"),
          extensions: ["png", "jpg", "jpeg", "webp", "bmp"],
        },
      ],
    });
    if (typeof picked === "string") {
      setImgPath(picked);
      setImgName(picked.split(/[\\/]/).pop() || picked);
    }
  };

  const onReplaceImage = async () => {
    const id = requireDoc();
    if (!id) return;
    if (!imgPath) {
      pushToast("info", t("请选择一张图片"));
      return;
    }
    setBusy(true);
    try {
      const info = await replaceImage(id, currentPage, objIndex, imgPath);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(id));
      setCanRedo(await canRedo(id));
      pushToast("info", t("已替换图片"));
      closeTask();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const onDeleteImage = async () => {
    const id = requireDoc();
    if (!id) return;
    if (!confirm(t("删除第 {n} 页的对象 {i}？", { n: currentPage + 1, i: objIndex }))) {
      return;
    }
    setBusy(true);
    try {
      const info = await deleteImageObject(id, currentPage, objIndex);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(id));
      setCanRedo(await canRedo(id));
      pushToast("info", t("已删除图片"));
      closeTask();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const onClearPageText = async () => {
    const id = requireDoc();
    if (!id) return;
    if (!confirm(t("清空第 {n} 页所有文字对象？图片和其他对象保留。", { n: currentPage + 1 }))) {
      return;
    }
    setBusy(true);
    try {
      const info = await clearPageText(id, [currentPage]);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(id));
      setCanRedo(await canRedo(id));
      pushToast("info", t("已清空页面文字"));
      closeTask();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const onDetect = async () => {
    const id = requireDoc();
    if (!id) return;
    setDetecting(true);
    try {
      const scanned = await isScannedPage(id, currentPage);
      setScanState(scanned ? "scanned" : "not");
    } catch (e) {
      errorToast(e);
    } finally {
      setDetecting(false);
    }
  };

  return (
    <div className="task-body">
      <p className="placeholder">
        {t("第 {n} / {total} 页。", { n: currentPage + 1, total: pageCount || "?" })}
      </p>
      <div className="split-modes">
        <label className={`split-mode${mode === "rewrite" ? " active" : ""}`}>
          <input
            type="radio"
            checked={mode === "rewrite"}
            onChange={() => setMode("rewrite")}
            style={{ display: "none" }}
          />
          {t("文字重写")}
        </label>
        <label className={`split-mode${mode === "addtext" ? " active" : ""}`}>
          <input
            type="radio"
            checked={mode === "addtext"}
            onChange={() => setMode("addtext")}
            style={{ display: "none" }}
          />
          {t("新增文本")}
        </label>
        <label className={`split-mode${mode === "image" ? " active" : ""}`}>
          <input
            type="radio"
            checked={mode === "image"}
            onChange={() => setMode("image")}
            style={{ display: "none" }}
          />
          {t("图片")}
        </label>
        <label className={`split-mode${mode === "scandetect" ? " active" : ""}`}>
          <input
            type="radio"
            checked={mode === "scandetect"}
            onChange={() => setMode("scandetect")}
            style={{ display: "none" }}
          />
          {t("扫描版")}
        </label>
      </div>

      {scanState === "scanned" && (
        <div
          style={{
            background: "var(--danger-bg, rgba(255,80,80,0.15))",
            border: "1px solid var(--danger)",
            borderRadius: 4,
            padding: 8,
            marginBottom: 8,
            fontSize: 12,
          }}
        >
          {t("扫描版提示")}
        </div>
      )}

      {mode === "rewrite" && (
        <>
          <p
            className="placeholder"
            style={{
              fontSize: 11,
              background: "var(--accent-bg, rgba(0,103,192,0.08))",
              border: "1px solid var(--accent)",
              borderRadius: 4,
              padding: 8,
            }}
          >
            {t("操作说明：在画布上用鼠标拖拽矩形选中要重写的文字区域 → 松开后弹出输入框。")}
          </p>
          <p className="placeholder" style={{ fontSize: 11 }}>
            {t("文字重写操作会用白色矩形覆盖原文字区域，然后按指定字号插入新文字；字体缺失时会自动回退到系统中文字体。")}
          </p>
          <div className="field">
            <label>{t("矩形（PDF 点，左下原点）")}</label>
          </div>
          <div className="form-row">
            <label>{t("左")}</label>
            <input
              type="number"
              value={region.left}
              onChange={(e) => setRegion({ ...region, left: parseFloat(e.target.value) || 0 })}
              style={{ width: 70 }}
            />
            <label>{t("下")}</label>
            <input
              type="number"
              value={region.bottom}
              onChange={(e) => setRegion({ ...region, bottom: parseFloat(e.target.value) || 0 })}
              style={{ width: 70 }}
            />
          </div>
          <div className="form-row">
            <label>{t("右")}</label>
            <input
              type="number"
              value={region.right}
              onChange={(e) => setRegion({ ...region, right: parseFloat(e.target.value) || 0 })}
              style={{ width: 70 }}
            />
            <label>{t("上")}</label>
            <input
              type="number"
              value={region.top}
              onChange={(e) => setRegion({ ...region, top: parseFloat(e.target.value) || 0 })}
              style={{ width: 70 }}
            />
          </div>
          <div className="task-footer">
            <button
              className="btn-primary"
              onClick={() =>
                onRewrite({
                  region,
                  pageIndex: currentPage,
                  newText,
                  fontSize: rwFontSize,
                  color: rwColor,
                })
              }
              disabled={busy || !newText.trim() || docId === null}
            >
              {busy ? t("处理中…") : t("确认重写")}
            </button>
          </div>
        </>
      )}

      {mode === "addtext" && (
        <>
          <p className="placeholder" style={{ fontSize: 11 }}>
            {t("新增文本框：在指定坐标插入一段新文本，不影响其他对象。")}
          </p>
          <div className="form-row">
            <label>{t("新文本内容")}</label>
            <input
              type="text"
              value={textInput}
              onChange={(e) => setTextInput(e.target.value)}
              placeholder={t("如：批注文字")}
            />
          </div>
          <div className="form-row">
            <label>{t("字号（pt）")}</label>
            <input
              type="number"
              min={8}
              max={200}
              value={tbFontSize}
              onChange={(e) =>
                setTbFontSize(Math.min(200, Math.max(8, parseInt(e.target.value) || 24)))
              }
              style={{ width: 64 }}
            />
            <label>{t("颜色")}</label>
            <input
              type="color"
              value={tbColor}
              onChange={(e) => setTbColor(e.target.value)}
              style={{ width: 40, padding: 0, height: 28 }}
            />
          </div>
          <div className="form-row">
            <label>{t("X 坐标（pt）")}</label>
            <input
              type="number"
              value={tbX}
              onChange={(e) => setTbX(parseFloat(e.target.value) || 0)}
              style={{ width: 90 }}
            />
          </div>
          <div className="form-row">
            <label>{t("Y 坐标（pt）")}</label>
            <input
              type="number"
              value={tbY}
              onChange={(e) => setTbY(parseFloat(e.target.value) || 0)}
              style={{ width: 90 }}
            />
          </div>
          <div className="task-footer">
            <button
              className="btn-primary"
              onClick={onAddText}
              disabled={busy || !textInput.trim() || docId === null}
            >
              {busy ? t("添加中…") : t("添加文本框")}
            </button>
          </div>
        </>
      )}

      {mode === "image" && (
        <>
          <p className="placeholder" style={{ fontSize: 11 }}>
            {t("替换/删除图片：替换整张图片或仅删除该图片对象。")}
          </p>
          <div className="form-row">
            <label>{t("对象索引")}</label>
            <input
              type="number"
              min={0}
              value={objIndex}
              onChange={(e) => setObjIndex(Math.max(0, parseInt(e.target.value) || 0))}
              style={{ width: 80 }}
            />
          </div>
          <div className="merge-output">
            <button onClick={pickImage} disabled={busy}>
              {t("替换为新图片…")}
            </button>
            <span className="merge-output-path" title={imgPath ?? undefined}>
              {imgName || t("未选择")}
            </span>
            {imgPath && (
              <button onClick={() => { setImgPath(null); setImgName(""); }} disabled={busy}>
                ✕
              </button>
            )}
          </div>
          <div className="task-footer">
            <button
              className="btn-primary"
              onClick={onReplaceImage}
              disabled={busy || !imgPath || docId === null}
            >
              {busy ? t("替换中…") : t("替换图片")}
            </button>
            <button
              onClick={onDeleteImage}
              disabled={busy || docId === null}
              style={{ marginLeft: 6, color: "var(--danger)" }}
            >
              {busy ? t("删除中…") : t("删除图片")}
            </button>
          </div>
          <button
            onClick={onClearPageText}
            disabled={busy || docId === null}
            style={{ marginTop: 12, color: "var(--danger)" }}
          >
            {t("清空页面文字（仅当前页）")}
          </button>
        </>
      )}

      {mode === "scandetect" && (
        <>
          <p className="placeholder" style={{ fontSize: 11 }}>
            {t("扫描版检测：判断页面是否几乎没有可识别文本（无文本层）。")}
          </p>
          <div className="task-footer">
            <button
              className="btn-primary"
              onClick={onDetect}
              disabled={detecting || docId === null}
            >
              {detecting ? t("正在检测…") : t("已扫描检测")}
            </button>
          </div>
          {scanState !== "unknown" && (
            <div style={{ marginTop: 12, fontSize: 13 }}>
              {t("本页面是否为扫描版")}：
              <span
                style={{
                  color: scanState === "scanned" ? "var(--danger)" : "var(--accent)",
                  fontWeight: 600,
                  marginLeft: 6,
                }}
              >
                {scanState === "scanned" ? t("是") : t("否")}
              </span>
            </div>
          )}
        </>
      )}
    </div>
  );

  // 文字重写 modal：在用户从画布完成选区后弹出
  if (rwModal) {
    const modal = rwModal!; // narrowed by `if (rwModal)`
      return (
        <RewriteModal
          region={modal.region}
          pageIndex={modal.pageIndex}
          onCancel={() => setRwModal(null)}
          onSubmit={(opts) => {
            onRewrite({
              region: modal.region,
              pageIndex: modal.pageIndex,
              newText: opts.newText,
              fontSize: opts.fontSize,
              color: opts.color,
            });
          }}
          busy={busy}
        />
      );
    }
  }

function RewriteModal({
  region,
  pageIndex,
  onCancel,
  onSubmit,
  busy,
}: {
  region: { left: number; bottom: number; right: number; top: number };
  pageIndex: number;
  onCancel: () => void;
  onSubmit: (opts: { newText: string; fontSize: number; color: string }) => void;
  busy: boolean;
}) {
  const t = useT();
  const [text, setText] = useState("");
  const [fontSize, setFontSize] = useState(24);
  const [color, setColor] = useState("#000000");
  const width = region.right - region.left;
  const height = region.top - region.bottom;
  return (
    <div className="task-body">
      <h3 style={{ margin: "0 0 12px 0" }}>
        {t("已选区 · 第 {n} 页", { n: pageIndex + 1 })}
      </h3>
      <p className="placeholder" style={{ fontSize: 12 }}>
        {t("矩形")}：{region.left.toFixed(1)}, {region.bottom.toFixed(1)} – {region.right.toFixed(1)}, {region.top.toFixed(1)}
        {" · "}
        {width.toFixed(1)}×{height.toFixed(1)} pt
      </p>
      <div className="form-row">
        <label>{t("新文本内容")}</label>
        <input
          type="text"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder={t("如：修正后的标题")}
          autoFocus
        />
      </div>
      <div className="form-row">
        <label>{t("字号（pt）")}</label>
        <input
          type="number"
          min={8}
          max={200}
          value={fontSize}
          onChange={(e) =>
            setFontSize(Math.min(200, Math.max(8, parseInt(e.target.value) || 24)))
          }
          style={{ width: 64 }}
        />
        <label>{t("颜色")}</label>
        <input
          type="color"
          value={color}
          onChange={(e) => setColor(e.target.value)}
          style={{ width: 40, padding: 0, height: 28 }}
        />
      </div>
      <div className="task-footer">
        <button
          className="btn-primary"
          onClick={() => onSubmit({ newText: text, fontSize, color })}
          disabled={busy || !text.trim()}
        >
          {busy ? t("处理中…") : t("确认重写")}
        </button>
        <button onClick={onCancel} disabled={busy} style={{ marginLeft: 6 }}>
          {t("取消")}
        </button>
      </div>
    </div>
  );
}

interface PermRow {
  key: keyof Omit<SecurityStatus, "handlerRevision">;
  labelKey: string;
}

const PERM_ROWS: PermRow[] = [
  { key: "canPrintHighQuality", labelKey: "高质量打印" },
  { key: "canPrintLowQuality", labelKey: "低质量打印" },
  { key: "canModifyDocument", labelKey: "修改文档内容" },
  { key: "canExtractTextAndGraphics", labelKey: "抽取文本与图形" },
  { key: "canAddAnnotations", labelKey: "添加或修改注释" },
  { key: "canFillFormFields", labelKey: "填写表单字段" },
  { key: "canAssembleDocument", labelKey: "组装文档（插页/旋转/删页等）" },
  { key: "canCreateNewFormFields", labelKey: "创建新表单字段" },
];

function SecurityPanel() {
  const docId = useApp((s) => s.docId);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const setCanUndo = useApp((s) => s.setCanUndo);
  const setCanRedo = useApp((s) => s.setCanRedo);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const t = useT();

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
      title: t("导出明文副本"),
      defaultPath: "plain.pdf",
      filters: [{ name: t("PDF 文档"), extensions: ["pdf"] }],
    });
    if (!p) return;
    setBusy(true);
    try {
      await exportPlainCopy(docId, p);
      // 同时把内存里的 doc 也去掉加密
      const info = await reloadPlain(docId);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId)); setCanRedo(await canRedo(docId));
      pushToast("info", t("已导出明文副本：{path}", { path: p }));
      await reload();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const onStripInMemory = async () => {
    if (docId === null) return;
    if (!confirm(t("去除当前文档的密码/加密？文件需另存到磁盘生效。"))) return;
    setBusy(true);
    try {
      const info = await reloadPlain(docId);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId)); setCanRedo(await canRedo(docId));
      pushToast("info", t("已去除内存中的加密，请立即 Ctrl+S 另存"));
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
        {t("pdfium-render 仅支持读取文档的加密状态与权限矩阵，不支持修改加密设置。加密文档可以导出为明文副本，或在内存中去加密后另存为。")}
      </p>

      {loading && <p className="placeholder">{t("加载中…")}</p>}
      {status && (
        <>
          <div className="security-status">
            <span className="security-status-label">{t("加密状态")}</span>
            <span
              className={`security-badge ${isProtected ? "protected" : "unprotected"}`}
            >
              {isProtected
                ? t("已加密（{handler}）", { handler: status.handlerRevision })
                : t("未加密")}
            </span>
          </div>

          <div className="security-perms">
            {PERM_ROWS.map((row) => {
              const ok = status[row.key];
              return (
                <div key={row.key} className="perm-row">
                  <span className={`perm-dot ${ok ? "ok" : "deny"}`} />
                  <span className="perm-label">{t(row.labelKey)}</span>
                  <span className="perm-result">{ok ? t("允许") : t("禁止")}</span>
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
              {busy ? t("导出中…") : isProtected ? t("导出明文副本") : t("导出当前文档")}
            </button>
            {isProtected && (
              <button
                onClick={onStripInMemory}
                disabled={busy}
                style={{ marginLeft: 6 }}
              >
                {t("在内存中去除加密")}
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
}

function ConvertPanel() {
  const docId = useApp((s) => s.docId);
  const currentPage = useApp((s) => s.currentPage);
  const pageCount = useApp((s) => s.pageCount);
  const selectedPages = useApp((s) => s.selectedPages);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const t = useT();

  const [mode, setMode] = useState<"pdf2img" | "img2pdf" | "office2pdf" | "ebook2pdf">("pdf2img");
  const [imgPaths, setImgPaths] = useState<string[]>([]);
  const [dpi, setDpi] = useState(150);
  const [format, setFormat] = useState<"png" | "jpeg">("png");
  const [pageSize, setPageSize] = useState<"fit" | "a4" | "letter" | "auto">("a4");
  const [layout, setLayout] = useState<"fit" | "fill">("fit");
  const [busy, setBusy] = useState(false);
  const [pagesStr, setPagesStr] = useState("");

  const effectivePages = (): number[] => {
    const txt = pagesStr.trim();
    if (selectedPages.size > 0 && !txt) {
      return Array.from(selectedPages).sort((a, b) => a - b);
    }
    if (!txt) return [currentPage];
    // 简单解析 "1,3,5-7"
    const out: number[] = [];
    txt.split(",").forEach((part) => {
      const p = part.trim();
      if (!p) return;
      if (p.includes("-")) {
        const [a, b] = p.split("-").map((x) => parseInt(x.trim()));
        if (!isNaN(a) && !isNaN(b)) {
          for (let i = Math.min(a, b); i <= Math.max(a, b); i++) out.push(i - 1);
        }
      } else {
        const v = parseInt(p);
        if (!isNaN(v)) out.push(v - 1);
      }
    });
    return out;
  };

  const onPdf2Img = async () => {
    if (docId === null) return;
    const pages = effectivePages().filter((p) => p >= 0 && p < pageCount);
    if (pages.length === 0) {
      pushToast("info", t("请指定至少一页"));
      return;
    }
    const outDir = await open({ title: t("选择输出目录"), directory: true });
    if (typeof outDir !== "string") return;
    setBusy(true);
    try {
      const outputs = await exportPagesToImages(
        docId,
        { pages, dpi, format },
        outDir,
      );
      pushToast("info", t("已导出 {n} 张图片", { n: outputs.length }));
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const addImages = async () => {
    const picked = await open({
      multiple: true,
      filters: [
        {
          name: t("图片"),
          extensions: ["png", "jpg", "jpeg", "bmp", "webp"],
        },
      ],
    });
    if (Array.isArray(picked)) {
      setImgPaths((prev) => [...prev, ...picked]);
    }
  };

  const onImg2Pdf = async () => {
    if (imgPaths.length === 0) {
      pushToast("info", t("请添加至少一张图片"));
      return;
    }
    const outPath = await save({
      title: t("图片另存为 PDF"),
      defaultPath: "images.pdf",
      filters: [{ name: t("PDF 文档"), extensions: ["pdf"] }],
    });
    if (typeof outPath !== "string") return;
    setBusy(true);
    try {
      const p = await imagesToPdf(
        { imagePaths: imgPaths, pageSize, layout },
        outPath,
      );
      pushToast("info", t("已生成 PDF：{path}（{n} 页）", { path: p, n: imgPaths.length }));
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="task-body">
      <div className="split-modes">
        <label className={`split-mode${mode === "pdf2img" ? " active" : ""}`}>
          <input
            type="radio"
            checked={mode === "pdf2img"}
            onChange={() => setMode("pdf2img")}
            style={{ display: "none" }}
          />
          {t("PDF → 图片")}
        </label>
        <label className={`split-mode${mode === "img2pdf" ? " active" : ""}`}>
          <input
            type="radio"
            checked={mode === "img2pdf"}
            onChange={() => setMode("img2pdf")}
            style={{ display: "none" }}
          />
          {t("图片 → PDF")}
        </label>
        <label className={`split-mode${mode === "office2pdf" ? " active" : ""}`}>
          <input
            type="radio"
            checked={mode === "office2pdf"}
            onChange={() => setMode("office2pdf")}
            style={{ display: "none" }}
          />
          {t("Office → PDF")}
        </label>
        <label className={`split-mode${mode === "ebook2pdf" ? " active" : ""}`}>
          <input
            type="radio"
            checked={mode === "ebook2pdf"}
            onChange={() => setMode("ebook2pdf")}
            style={{ display: "none" }}
          />
          {t("电子书 → PDF")}
        </label>
      </div>

      {mode === "pdf2img" ? (
        <>
          <div className="field">
            <label>{t("页码范围（留空 = 当前页，多个则逗号分隔，如 1,3,5-7）")}</label>
            <input
              type="text"
              value={pagesStr}
              onChange={(e) => setPagesStr(e.target.value)}
              placeholder={t("第 {n} 页", { n: currentPage + 1 })}
            />
            {selectedPages.size > 0 && (
              <p className="placeholder" style={{ marginTop: 4 }}>
                {t("已选中 {n} 页，留空将导出这些页", { n: selectedPages.size })}
              </p>
            )}
          </div>
          <div className="form-row">
            <label>{t("格式")}</label>
            <select
              value={format}
              onChange={(e) => setFormat(e.target.value as "png" | "jpeg")}
            >
              <option value="png">{t("PNG（无损）")}</option>
              <option value="jpeg">{t("JPEG（体积小）")}</option>
            </select>
          </div>
          <div className="form-row">
            <label>{t("DPI")}</label>
            <input
              type="number"
              min={36}
              max={600}
              value={dpi}
              onChange={(e) =>
                setDpi(Math.min(600, Math.max(36, parseInt(e.target.value) || 150)))
              }
              style={{ width: 80 }}
            />
            <label>{t("（36–600）")}</label>
          </div>
          <div className="task-footer">
            <button
              className="btn-primary"
              onClick={onPdf2Img}
              disabled={busy || docId === null}
            >
              {busy ? t("导出中…") : t("导出图片")}
            </button>
          </div>
        </>
      ) : (
        <>
          <div className="merge-output">
            <button onClick={addImages} disabled={busy}>
              {t("添加图片…")}
            </button>
            <span
              className="merge-output-path"
              style={imgPaths.length ? undefined : { color: "var(--fg-dim)" }}
              title={imgPaths.join("\n")}
            >
              {imgPaths.length > 0
                ? t("已选 {n} 张", { n: imgPaths.length })
                : t("未选择")}
            </span>
            {imgPaths.length > 0 && (
              <button onClick={() => setImgPaths([])} disabled={busy}>
                {t("清空")}
              </button>
            )}
          </div>
          <div className="form-row">
            <label>{t("页面尺寸")}</label>
            <select
              value={pageSize}
              onChange={(e) =>
                setPageSize(e.target.value as "fit" | "a4" | "letter" | "auto")
              }
            >
              <option value="fit">{t("按图片（每页不同）")}</option>
              <option value="a4">{t("统一 A4")}</option>
              <option value="letter">{t("统一 Letter")}</option>
              <option value="auto">{t("取最大")}</option>
            </select>
          </div>
          <div className="form-row">
            <label>{t("布局")}</label>
            <select
              value={layout}
              onChange={(e) => setLayout(e.target.value as "fit" | "fill")}
            >
              <option value="fit">{t("按比例居中（推荐）")}</option>
              <option value="fill">{t("拉伸铺满")}</option>
            </select>
          </div>
          <div className="task-footer">
            <button
              className="btn-primary"
              onClick={onImg2Pdf}
              disabled={busy || imgPaths.length === 0}
            >
              {busy ? t("生成中…") : t("生成 PDF")}
            </button>
          </div>
        </>
      )}

      {mode === "office2pdf" && (
        <Office2PdfSection busy={busy} setBusy={setBusy} pushToast={pushToast} errorToast={errorToast} t={t} />
      )}

      {mode === "ebook2pdf" && (
        <Ebook2PdfSection busy={busy} setBusy={setBusy} pushToast={pushToast} errorToast={errorToast} t={t} />
      )}
    </div>
  );
}

function Office2PdfSection({
  busy,
  setBusy,
  pushToast,
  errorToast,
  t,
}: {
  busy: boolean;
  setBusy: (b: boolean) => void;
  pushToast: (k: "info" | "error", msg: string) => void;
  errorToast: (e: unknown) => void;
  t: (zh: string, vars?: Record<string, string | number>) => string;
}) {
  const [probe, setProbe] = useState<OfficeProbe | null>(null);
  const [probing, setProbing] = useState(false);
  const [src, setSrc] = useState<string | null>(null);
  const [srcName, setSrcName] = useState("");

  const runProbe = async () => {
    setProbing(true);
    try {
      setProbe(await detectOffice());
    } catch (e) {
      errorToast(e);
    } finally {
      setProbing(false);
    }
  };

  const pickFile = async () => {
    const picked = await open({
      multiple: false,
      filters: [
        {
          name: t("Office 文档"),
          extensions: ["doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf"],
        },
      ],
    });
    if (typeof picked === "string") {
      setSrc(picked);
      setSrcName(picked.split(/[\\/]/).pop() || picked);
    }
  };

  const convert = async () => {
    if (!probe?.installed || !probe.path || !src) {
      pushToast("info", t("请先探测 LibreOffice 并选择源文件"));
      return;
    }
    const outDir = await open({ title: t("选择输出目录"), directory: true });
    if (typeof outDir !== "string") return;
    setBusy(true);
    try {
      const p = await convertOfficeToPdf(probe.path, src, outDir);
      pushToast("info", t("已生成 PDF：{path}", { path: p }));
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="merge-output">
        <button onClick={runProbe} disabled={probing || busy}>
          {probing ? t("探测中…") : probe ? t("重新探测") : t("探测 LibreOffice")}
        </button>
        {probe && (
          probe.installed ? (
            <span
              className="merge-output-path"
              title={probe.path ?? undefined}
            >
              ✓ {probe.version ?? t("已安装")}
            </span>
          ) : (
            <span
              className="merge-output-path"
              style={{ color: "var(--danger)" }}
            >
              {t("未安装 LibreOffice，请先下载安装（libreoffice.org）")}
            </span>
          )
        )}
      </div>
      <div className="merge-output">
        <button onClick={pickFile} disabled={busy}>
          {t("选择 Office 文件…")}
        </button>
        <span
          className="merge-output-path"
          title={src ?? undefined}
          style={src ? undefined : { color: "var(--fg-dim)" }}
        >
          {srcName || t("未选择")}
        </span>
        {src && (
          <button onClick={() => { setSrc(null); setSrcName(""); }} disabled={busy}>
            ✕
          </button>
        )}
      </div>
      <p className="placeholder" style={{ marginTop: 8, fontSize: 11 }}>
        {t("支持 .doc / .docx / .xls / .xlsx / .ppt / .pptx / .odt / .ods / .odp / .rtf 转换为 PDF。PDF → Office 不支持（请使用专业工具）。")}
      </p>
      <div className="task-footer">
        <button
          className="btn-primary"
          onClick={convert}
          disabled={busy || !probe?.installed || !src}
        >
          {busy ? t("转换中…") : t("转换为 PDF")}
        </button>
      </div>
    </>
  );
}

function Ebook2PdfSection({
  busy,
  setBusy,
  pushToast,
  errorToast,
  t,
}: {
  busy: boolean;
  setBusy: (b: boolean) => void;
  pushToast: (k: "info" | "error", msg: string) => void;
  errorToast: (e: unknown) => void;
  t: (zh: string, vars?: Record<string, string | number>) => string;
}) {
  const [probe, setProbe] = useState<EbookToolProbe | null>(null);
  const [probing, setProbing] = useState(false);
  const [src, setSrc] = useState<string | null>(null);
  const [srcName, setSrcName] = useState("");
  const [title, setTitle] = useState("");
  const [author, setAuthor] = useState("");

  const runProbe = async () => {
    setProbing(true);
    try {
      setProbe(await detectEbookTools());
    } catch (e) {
      errorToast(e);
    } finally {
      setProbing(false);
    }
  };

  const pickFile = async () => {
    const picked = await open({
      multiple: false,
      filters: [
        {
          name: t("电子书"),
          extensions: [
            "epub", "mobi", "azw", "azw3", "fb2", "lit",
            "html", "htm", "rtf", "odt", "docx", "txt",
          ],
        },
      ],
    });
    if (typeof picked === "string") {
      setSrc(picked);
      setSrcName(picked.split(/[\\/]/).pop() || picked);
    }
  };

  const convert = async () => {
    if (!probe?.installed || !probe.path || !src) {
      pushToast("info", t("请先探测 Calibre 并选择源文件"));
      return;
    }
    const outPath = await save({
      title: t("电子书另存为 PDF"),
      defaultPath: "ebook.pdf",
      filters: [{ name: t("PDF 文档"), extensions: ["pdf"] }],
    });
    if (typeof outPath !== "string") return;
    setBusy(true);
    try {
      const p = await convertEbookToPdf(probe.path, {
        source: src,
        output: outPath,
        title: title.trim() || null,
        author: author.trim() || null,
      });
      pushToast("info", t("已生成 PDF：{path}", { path: p }));
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="merge-output">
        <button onClick={runProbe} disabled={probing || busy}>
          {probing ? t("探测中…") : probe ? t("重新探测") : t("探测 Calibre")}
        </button>
        {probe &&
          (probe.installed ? (
            <span className="merge-output-path" title={probe.path ?? undefined}>
              ✓ {probe.version ?? t("已安装")}
            </span>
          ) : (
            <span className="merge-output-path" style={{ color: "var(--danger)" }}>
              {t("未安装 Calibre，请先下载安装（calibre-ebook.com）")}
            </span>
          ))}
      </div>
      <div className="merge-output">
        <button onClick={pickFile} disabled={busy}>
          {t("选择电子书…")}
        </button>
        <span
          className="merge-output-path"
          title={src ?? undefined}
          style={src ? undefined : { color: "var(--fg-dim)" }}
        >
          {srcName || t("未选择")}
        </span>
        {src && (
          <button
            onClick={() => {
              setSrc(null);
              setSrcName("");
            }}
            disabled={busy}
          >
            ✕
          </button>
        )}
      </div>
      <div className="form-row">
        <label>{t("标题")}</label>
        <input
          type="text"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={t("可选，写入 PDF 元数据")}
        />
      </div>
      <div className="form-row">
        <label>{t("作者")}</label>
        <input
          type="text"
          value={author}
          onChange={(e) => setAuthor(e.target.value)}
          placeholder={t("可选，写入 PDF 元数据")}
        />
      </div>
      <p className="placeholder" style={{ marginTop: 8, fontSize: 11 }}>
        {t("支持 EPUB / MOBI / AZW / AZW3 / FB2 / LIT / HTML / RTF / ODT / DOCX / TXT 转换为 PDF。需本机安装 Calibre（含 ebook-convert）。")}
      </p>
      <div className="task-footer">
        <button
          className="btn-primary"
          onClick={convert}
          disabled={busy || !probe?.installed || !src}
        >
          {busy ? t("转换中…") : t("转换为 PDF")}
        </button>
      </div>
    </>
  );
}

export default function TaskPanel() {
  const task = useApp((s) => s.task);
  const closeTask = useApp((s) => s.closeTask);
  const t = useT();

  const TITLES: Record<Exclude<TaskId, null>, string> = {
    merge: t("合并文档"),
    split: t("拆分文档"),
    watermark: t("水印"),
    edit: t("内容编辑"),
    security: t("文档安全"),
    export: t("导出图片"),
  };

  const DESC: Record<Exclude<TaskId, null>, string> = {
    merge: t("将多个 PDF 按顺序合并为一个文档，可对每个文件选择页码范围。"),
    split: t("按固定页数、自定义范围或书签层级，将文档拆分为多个文件。"),
    watermark: t("为页面添加文字或图片水印，支持位置、透明度与平铺。"),
    edit: t("为当前页添加 PDF 注释：高亮、下划线、删除线、便签、自由文本框、矩形标注。"),
    security: t("查看文档加密状态与权限矩阵；导出明文副本或在内存中去除加密后另存。"),
    export: t("PDF 与图片互转：PDF → PNG/JPEG（按页可调 DPI）；PNG/JPG/JPEG/BMP/WebP → PDF（多图合并）。"),
  };

  if (task === null) return null;

  return (
    <aside className="task">
      <h3>
        <span>{TITLES[task]}</span>
        <button title={t("关闭面板")} onClick={closeTask}>
          ✕
        </button>
      </h3>
      <div className="body">
        {task === "merge" && <MergePanel />}
        {task === "split" && <SplitPanel />}
        {task === "watermark" && <WatermarkPanel />}
        {task === "edit" && <EditPanel />}
        {task === "security" && <SecurityPanel />}
        {task === "export" && <ConvertPanel />}
        {task !== "merge" && task !== "split" && task !== "watermark" && task !== "edit" && task !== "security" && task !== "export" && (
          <>
            <p className="placeholder">{DESC[task]}</p>
            <p className="placeholder" style={{ marginTop: 12 }}>
              {t("该功能将在后续里程碑（M4–M6）中交付。")}
            </p>
          </>
        )}
      </div>
    </aside>
  );
}


