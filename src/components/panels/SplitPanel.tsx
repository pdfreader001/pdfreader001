import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useApp } from "../../state/store";
import { useT } from "../../i18n";
import { splitDocument } from "../../lib/ipc";
import type { SplitMode } from "../../lib/ipc";

export default function SplitPanel() {
  const docId = useApp((s) => s.docId);
  const pageCount = useApp((s) => s.pageCount);
  const selectedPages = useApp((s) => s.selectedPages);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const t = useT();

  const [mode, setMode] = useState<
    "every_n" | "ranges" | "by_bookmark" | "selected"
  >("every_n");
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
        modePayload = {
          mode: "by_bookmark",
          payload: { level: bookmarkLevel },
        };
        break;
      case "selected":
        if (selectedPages.size === 0) {
          pushToast("info", t("请先在缩略图中选择页面"));
          return;
        }
        modePayload = {
          mode: "selected",
          payload: { pages: Array.from(selectedPages) },
        };
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
      pushToast(
        "info",
        t("拆分完成，共生成 {n} 个文件", { n: outputs.length }),
      );
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
          {t("已选择 {n} 页，每页将单独保存为一个文件。", {
            n: selectedPages.size,
          })}
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
