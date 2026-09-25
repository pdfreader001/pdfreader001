import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useApp } from "../../state/store";
import { useT } from "../../i18n";
import {
  addTextWatermark,
  addImageWatermark,
  removeObjectsInRect,
  detectWatermarkCandidates,
  applyWatermarkRemoval,
  refreshUndoRedo,
} from "../../lib/ipc";
import type { WatermarkStyle } from "../../lib/ipc";
import type { DetectResult, ObjectFingerprint, Rect as RemoveRect } from "../../lib/ipc";

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

export default function WatermarkPanel() {
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
  const setUndoRedo = useApp((s) => s.setUndoRedo);
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
      filters: [{ name: t("图片"), extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }],
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
      setUndoRedo(await refreshUndoRedo(docId));
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
              onChange={(e) => setScale(Math.min(100, Math.max(5, parseInt(e.target.value) || 30)))}
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
        <span style={{ width: 34, textAlign: "right", color: "var(--fg-dim)" }}>{opacity}%</span>
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
          <input type="checkbox" checked={tiled} onChange={(e) => setTiled(e.target.checked)} />
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
              onChange={(e) => setTileSpacing(Math.max(20, parseInt(e.target.value) || 120))}
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
        <button className="btn-primary" onClick={apply} disabled={busy || docId === null || !valid}>
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
  const pages = useApp((s) => s.pages);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const setUndoRedo = useApp((s) => s.setUndoRedo);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const closeTask = useApp((s) => s.closeTask);
  const selectingFor = useApp((s) => s.selectingFor);
  const setSelectingFor = useApp((s) => s.setSelectingFor);
  const completedSelection = useApp((s) => s.completedSelection);
  const setCompletedSelection = useApp((s) => s.setCompletedSelection);
  const t = useT();

  type SubMode = "manual" | "auto";
  const [subMode, setSubMode] = useState<SubMode>("manual");
  const [busy, setBusy] = useState(false);
  const [rect, setRect] = useState<RemoveRect>({
    left: 100,
    bottom: 100,
    right: 300,
    top: 200,
  });
  const [onlyCurrent, setOnlyCurrent] = useState(true);
  const [samplePages, setSamplePages] = useState(10);
  const [threshold, setThreshold] = useState(0.6);
  const [detectResult, setDetectResult] = useState<DetectResult | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [selectedFp, setSelectedFp] = useState<Set<string>>(new Set());

  const requireDoc = (): number | null => {
    if (docId === null) {
      pushToast("info", t("请打开文档后再操作"));
      return null;
    }
    return docId;
  };

  useEffect(() => {
    if (!completedSelection || completedSelection.mode !== "watermarkRemove" || !pages.length)
      return;
    const sel = completedSelection;
    const pageInfo = pages[sel.pageIndex];
    if (!pageInfo) return;
    const s = sel.scale;
    if (s <= 0) return;
    const { rect: r } = sel;
    const pageH_pt = pageInfo.height;
    const leftPdf = r.left / s;
    const rightPdf = r.right / s;
    const topPdf = pageH_pt - r.top / s;
    const bottomPdf = pageH_pt - r.bottom / s;
    setRect({
      left: Math.round(leftPdf * 10) / 10,
      right: Math.round(rightPdf * 10) / 10,
      top: Math.round(topPdf * 10) / 10,
      bottom: Math.round(bottomPdf * 10) / 10,
    });
    setOnlyCurrent(true);
    setCompletedSelection(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [completedSelection]);

  const validRect = rect.right > rect.left && rect.top > rect.bottom;

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
      setUndoRedo(await refreshUndoRedo(id));
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
      setSelectedFp(new Set(res.candidates.map((c) => c.key ?? `idx:${c.objectIndex}`)));
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
    const keys = Array.from(selectedFp);
    setBusy(true);
    try {
      const res = await applyWatermarkRemoval(id, keys);
      updatePages(res.info);
      markDirty(true);
      setUndoRedo(await refreshUndoRedo(id));
      pushToast("info", t("已删除 {n} 个对象", { n: res.removedCount }));
      closeTask();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const toggleFp = (key: string) => {
    setSelectedFp((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  return (
    <div className="task-body">
      <p className="placeholder">{t("在当前页用鼠标框选区域，或自动检测重复水印对象")}</p>
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
          <div className="field" style={{ marginBottom: 8 }}>
            <button
              className="btn-ghost"
              onClick={() => {
                if (selectingFor === "watermarkRemove") {
                  setSelectingFor(null);
                } else {
                  setSelectingFor("watermarkRemove");
                }
              }}
              disabled={docId === null}
              style={{ width: "100%" }}
            >
              {selectingFor === "watermarkRemove" ? t("取消框选") : t("🎯 在画布上框选水印区域")}
            </button>
          </div>
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
                  {detectResult.candidates.map((fp) => {
                    const k = fp.key ?? `idx:${fp.objectIndex}`;
                    return (
                      <WatermarkCandidateRow
                        key={k}
                        fp={fp}
                        totalSampled={detectResult.sampledPages}
                        checked={selectedFp.has(k)}
                        onToggle={() => toggleFp(k)}
                      />
                    );
                  })}
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
        {t("类型")}: {fp.kind} · {t("位置")}: ({fp.left.toFixed(0)}, {fp.bottom.toFixed(0)}) – (
        {fp.right.toFixed(0)}, {fp.top.toFixed(0)}) · {t("出现")}: {fp.occurrence}/{totalSampled}
      </span>
    </label>
  );
}
