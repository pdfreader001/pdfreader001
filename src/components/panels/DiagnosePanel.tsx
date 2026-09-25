import { useEffect, useState } from "react";
import { useApp } from "../../state/store";
import { useT } from "../../i18n";
import { getSecurityStatus, isScannedPage } from "../../lib/ipc";
import type { SecurityStatus } from "../../lib/ipc";

const cellStyle: React.CSSProperties = {
  padding: "6px 8px",
  borderBottom: "1px solid var(--border)",
  color: "var(--text-dim)",
  width: "33%",
};
const valStyle: React.CSSProperties = {
  padding: "6px 8px",
  borderBottom: "1px solid var(--border)",
  fontWeight: 500,
};

export default function DiagnosePanel() {
  const docId = useApp((s) => s.docId);
  const fileName = useApp((s) => s.fileName);
  const fileSizeBytes = useApp((s) => s.fileSizeBytes);
  const pageCount = useApp((s) => s.pageCount);
  const annotations = useApp((s) => s.annotations);
  const errorToast = useApp((s) => s.errorToast);
  const t = useT();

  const [security, setSecurity] = useState<SecurityStatus | null>(null);
  const [scan, setScan] = useState<
    | { sampled: number; scannedCount: number; ratio: number; running: boolean }
    | null
  >(null);

  const annotationTotal = Object.values(annotations).reduce(
    (s, arr) => s + arr.length,
    0,
  );

  const reloadSecurity = async () => {
    if (docId === null) return;
    try {
      const s = await getSecurityStatus(docId);
      setSecurity(s);
    } catch (e) {
      errorToast(e);
    }
  };

  const runScanSample = async () => {
    if (docId === null) return;
    setScan({ sampled: 0, scannedCount: 0, ratio: 0, running: true });
    try {
      const total = pageCount;
      const sampleSize = Math.min(10, total);
      const step = total <= sampleSize ? 1 : Math.floor(total / sampleSize);
      const indices: number[] = [];
      for (let i = 0; i < sampleSize; i++) {
        indices.push(Math.min(i * step, total - 1));
      }
      let scannedCount = 0;
      for (const idx of indices) {
        if (await isScannedPage(docId, idx)) scannedCount++;
      }
      setScan({
        sampled: indices.length,
        scannedCount,
        ratio: indices.length > 0 ? scannedCount / indices.length : 0,
        running: false,
      });
    } catch (e) {
      errorToast(e);
      setScan(null);
    }
  };

  useEffect(() => {
    setSecurity(null);
    setScan(null);
    reloadSecurity();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [docId]);

  const fmtSize = (n: number): string => {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(2)} MB`;
  };

  const isProtected =
    security?.handlerRevision !== undefined &&
    security.handlerRevision !== "Unprotected" &&
    security.handlerRevision !== "Unknown";

  return (
    <div className="task-body">
      <p className="placeholder">
        {t(
          "查看文档关键统计：页数、文件大小、加密状态、注释总数、扫描版抽样。",
        )}
      </p>

      <table
        style={{
          width: "100%",
          borderCollapse: "collapse",
          fontSize: 13,
          marginBottom: 12,
        }}
      >
        <tbody>
          <tr>
            <td style={cellStyle}>{t("文件名")}</td>
            <td style={valStyle}>{fileName || "—"}</td>
          </tr>
          <tr>
            <td style={cellStyle}>{t("页数")}</td>
            <td style={valStyle}>{pageCount}</td>
          </tr>
          <tr>
            <td style={cellStyle}>{t("文件大小")}</td>
            <td style={valStyle}>{fmtSize(fileSizeBytes)}</td>
          </tr>
          <tr>
            <td style={cellStyle}>{t("加密状态")}</td>
            <td style={valStyle}>
              {security === null
                ? t("加载中…")
                : isProtected
                  ? t("已加密（{handler}）", {
                      handler: security!.handlerRevision,
                    })
                  : t("未加密")}
            </td>
          </tr>
          <tr>
            <td style={cellStyle}>{t("注释总数")}</td>
            <td style={valStyle}>{annotationTotal}</td>
          </tr>
          <tr>
            <td style={cellStyle}>{t("扫描检测")}</td>
            <td style={valStyle}>
              {scan === null
                ? t("未运行")
                : scan.running
                  ? t("检测中…")
                  : t("抽样 {n} 页，扫描版 {m} 页", {
                      n: scan.sampled,
                      m: scan.scannedCount,
                    })}
            </td>
          </tr>
        </tbody>
      </table>

      <div
        className="task-footer"
        style={{ display: "flex", gap: 8, flexWrap: "wrap" }}
      >
        <button onClick={reloadSecurity} disabled={docId === null}>
          {t("刷新加密状态")}
        </button>
        <button
          className="btn-primary"
          onClick={runScanSample}
          disabled={docId === null || (scan?.running ?? false)}
        >
          {scan?.running ? t("检测中…") : t("运行扫描抽样")}
        </button>
      </div>

      {scan && !scan.running && scan.ratio >= 0.5 && (
        <div
          style={{
            marginTop: 12,
            fontSize: 12,
            color: "var(--danger)",
            padding: 8,
            background: "var(--danger-bg, rgba(255,80,80,0.15))",
            border: "1px solid var(--danger)",
            borderRadius: 4,
          }}
        >
          {t("扫描版提示")}
        </div>
      )}
    </div>
  );
}
