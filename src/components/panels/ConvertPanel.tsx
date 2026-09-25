import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useApp } from "../../state/store";
import { useT } from "../../i18n";
import {
  exportPagesToImages,
  imagesToPdf,
  detectOffice,
  convertOfficeToPdf,
  detectEbookTools,
  convertEbookToPdf,
} from "../../lib/ipc";
import type { OfficeProbe, EbookToolProbe } from "../../lib/ipc";

export default function ConvertPanel() {
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
      const outputs = await exportPagesToImages(docId, { pages, dpi, format }, outDir);
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
      const p = await imagesToPdf({ imagePaths: imgPaths, pageSize, layout }, outPath);
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
            <select value={format} onChange={(e) => setFormat(e.target.value as "png" | "jpeg")}>
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
              onChange={(e) => setDpi(Math.min(600, Math.max(36, parseInt(e.target.value) || 150)))}
              style={{ width: 80 }}
            />
            <label>{t("（36–600）")}</label>
          </div>
          <div className="task-footer">
            <button className="btn-primary" onClick={onPdf2Img} disabled={busy || docId === null}>
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
              {imgPaths.length > 0 ? t("已选 {n} 张", { n: imgPaths.length }) : t("未选择")}
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
              onChange={(e) => setPageSize(e.target.value as typeof pageSize)}
            >
              <option value="fit">{t("按图片（每页不同）")}</option>
              <option value="a4">{t("统一 A4")}</option>
              <option value="letter">{t("统一 Letter")}</option>
              <option value="auto">{t("取最大")}</option>
            </select>
          </div>
          <div className="form-row">
            <label>{t("布局")}</label>
            <select value={layout} onChange={(e) => setLayout(e.target.value as typeof layout)}>
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

      {mode === "office2pdf" && <Office2PdfSection setBusy={setBusy} />}
      {mode === "ebook2pdf" && <Ebook2PdfSection setBusy={setBusy} />}
    </div>
  );
}

function Office2PdfSection({ setBusy }: { setBusy: (b: boolean) => void }) {
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const t = useT();

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
        <button onClick={runProbe} disabled={probing}>
          {probing ? t("探测中…") : probe ? t("重新探测") : t("探测 LibreOffice")}
        </button>
        {probe &&
          (probe.installed ? (
            <span className="merge-output-path" title={probe.path ?? undefined}>
              ✓ {probe.version ?? t("已安装")}
            </span>
          ) : (
            <span className="merge-output-path" style={{ color: "var(--danger)" }}>
              {t("未安装 LibreOffice，请先下载安装（libreoffice.org）")}
            </span>
          ))}
      </div>
      <div className="merge-output">
        <button onClick={pickFile} disabled={false}>
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
          <button
            onClick={() => {
              setSrc(null);
              setSrcName("");
            }}
            disabled={false}
          >
            ✕
          </button>
        )}
      </div>
      <p className="placeholder" style={{ marginTop: 8, fontSize: 11 }}>
        {t(
          "支持 .doc / .docx / .xls / .xlsx / .ppt / .pptx / .odt / .ods / .odp / .rtf 转换为 PDF。PDF → Office 不支持（请使用专业工具）。",
        )}
      </p>
      <div className="task-footer">
        <button className="btn-primary" onClick={convert} disabled={!probe?.installed || !src}>
          {t("转换为 PDF")}
        </button>
      </div>
    </>
  );
}

function Ebook2PdfSection({ setBusy }: { setBusy: (b: boolean) => void }) {
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const t = useT();

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
            "epub",
            "mobi",
            "azw",
            "azw3",
            "fb2",
            "lit",
            "html",
            "htm",
            "rtf",
            "odt",
            "docx",
            "txt",
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
        <button onClick={runProbe} disabled={probing}>
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
        <button onClick={pickFile}>{t("选择电子书…")}</button>
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
        {t(
          "支持 EPUB / MOBI / AZW / AZW3 / FB2 / LIT / HTML / RTF / ODT / DOCX / TXT 转换为 PDF。需本机安装 Calibre（含 ebook-convert）。",
        )}
      </p>
      <div className="task-footer">
        <button className="btn-primary" onClick={convert} disabled={!probe?.installed || !src}>
          {t("转换为 PDF")}
        </button>
      </div>
    </>
  );
}
