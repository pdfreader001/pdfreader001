import { useState } from "react";
import { useApp } from "../state/store";

export default function StatusBar() {
  const fileName = useApp((s) => s.fileName);
  const pageCount = useApp((s) => s.pageCount);
  const dirty = useApp((s) => s.dirty);
  const docId = useApp((s) => s.docId);
  const scale = useApp((s) => s.scale);
  const setScale = useApp((s) => s.setScale);
  const setFitMode = useApp((s) => s.setFitMode);
  const currentPage = useApp((s) => s.currentPage);
  const jumpToPage = useApp((s) => s.jumpToPage);
  const [pageText, setPageText] = useState("");

  if (docId === null) {
    return (
      <footer className="statusbar">
        <span>未打开文档</span>
        <span className="grow" />
      </footer>
    );
  }

  const commitPage = () => {
    const n = parseInt(pageText, 10);
    if (!Number.isNaN(n)) jumpToPage(n - 1);
    setPageText("");
  };

  return (
    <footer className="statusbar">
      <span title={fileName}>
        {fileName} · 共 {pageCount} 页{dirty ? " · 未保存" : ""}
      </span>
      <span className="grow" />
      <span>
        第
        <input
          className="pageinput"
          value={pageText || String(currentPage + 1)}
          onChange={(e) => setPageText(e.target.value)}
          onFocus={() => setPageText(String(currentPage + 1))}
          onBlur={commitPage}
          onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
        />
        / {pageCount} 页
      </span>
      <span className="zoomctl">
        <button onClick={() => setScale(scale / 1.2)} title="缩小">
          −
        </button>
        <button
          style={{ width: 52 }}
          onClick={() => setFitMode("width")}
          title="适应宽度"
        >
          {Math.round(scale * 100)}%
        </button>
        <button onClick={() => setScale(scale * 1.2)} title="放大">
          ＋
        </button>
        <button onClick={() => setFitMode("page")} title="适应页面">
          ⤢
        </button>
      </span>
    </footer>
  );
}
