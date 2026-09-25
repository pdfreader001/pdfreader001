import { useState } from "react";
import { useApp } from "../../state/store";
import { useT } from "../../i18n";
import {
  ocrPage,
  ocrApplyTextOverlay,
  refreshUndoRedo,
} from "../../lib/ipc";
import type { OcrWord } from "../../lib/ipc";

export default function OcrPanel() {
  const docId = useApp((s) => s.docId);
  const currentPage = useApp((s) => s.currentPage);
  const pageCount = useApp((s) => s.pageCount);
  const closeTask = useApp((s) => s.closeTask);
  const pushToast = useApp((s) => s.pushToast);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const errorToast = useApp((s) => s.errorToast);
  const setUndoRedo = useApp((s) => s.setUndoRedo);
  const t = useT();

  const [lang, setLang] = useState("eng");
  const [dpi, setDpi] = useState(300);
  const [busy, setBusy] = useState(false);
  const [lastResult, setLastResult] = useState<{
    words: OcrWord[];
    page: number;
  } | null>(null);

  const requireDoc = () => {
    if (docId === null) {
      errorToast(new Error("文档未打开"));
      return null;
    }
    return docId;
  };

  const onRecognize = async () => {
    const id = requireDoc();
    if (!id) return;
    setBusy(true);
    try {
      const result = await ocrPage(id, currentPage, lang, dpi);
      setLastResult({ words: result.words, page: currentPage });
      pushToast(
        "info",
        t("OCR 识别完成，共 {n} 个词", { n: result.words.length }),
      );
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const onApply = async () => {
    const id = requireDoc();
    if (!id || !lastResult || lastResult.page !== currentPage) {
      errorToast(new Error("请先运行 OCR 识别"));
      return;
    }
    setBusy(true);
    try {
      const info = await ocrApplyTextOverlay(
        id,
        currentPage,
        lastResult.words,
      );
      updatePages(info);
      markDirty(true);
      setUndoRedo(await refreshUndoRedo(id));
      pushToast("info", t("OCR 文本层已应用，请立即保存文档"));
      closeTask();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="task-body">
      <p className="placeholder" style={{ fontSize: 11 }}>
        {t(
          "对扫描版 PDF 调用 Tesseract 识别文字，结果作为不可见文本层写回，生成可搜索 PDF。需 cargo build --features ocr。",
        )}
      </p>
      <p className="placeholder">
        {t("第 {n} / {total} 页", {
          n: currentPage + 1,
          total: pageCount || "?",
        })}
      </p>
      <div className="form-row">
        <label>{t("语言代码")}</label>
        <input
          type="text"
          value={lang}
          onChange={(e) => setLang(e.target.value)}
          placeholder="eng / chi_sim / jpn_vert"
        />
      </div>
      <div className="form-row">
        <label>{t("DPI")}</label>
        <input
          type="number"
          min={72}
          max={600}
          value={dpi}
          onChange={(e) => setDpi(parseInt(e.target.value) || 300)}
        />
      </div>
      <div
        className="task-footer"
        style={{ display: "flex", gap: 8, flexWrap: "wrap" }}
      >
        <button
          className="btn-primary"
          onClick={onRecognize}
          disabled={busy || docId === null}
        >
          {busy ? t("识别中…") : t("运行 OCR")}
        </button>
        <button
          onClick={onApply}
          disabled={
            busy ||
            docId === null ||
            !lastResult ||
            lastResult.page !== currentPage
          }
        >
          {t("应用为可搜索文本层")}
        </button>
      </div>
      {lastResult && (
        <div
          style={{
            marginTop: 12,
            padding: 8,
            fontSize: 12,
            background: "var(--bg-soft)",
            border: "1px solid var(--border)",
            borderRadius: 4,
          }}
        >
          <div>
            {t("已识别 {n} 个词（页 {p}）", {
              n: lastResult.words.length,
              p: lastResult.page + 1,
            })}
          </div>
          {lastResult.words.length > 0 && (
            <div
              style={{
                marginTop: 4,
                color: "var(--text-dim)",
                fontSize: 11,
              }}
            >
              {lastResult.words
                .slice(0, 8)
                .map((w) => w.text)
                .join(" · ")}
              {lastResult.words.length > 8 ? " …" : ""}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
