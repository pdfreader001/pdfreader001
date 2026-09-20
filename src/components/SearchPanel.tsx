import { useEffect, useRef, useState } from "react";
import { useApp } from "../state/store";
import type { SearchHit } from "../state/store";
import { getPageText } from "../lib/ipc";
import { useT } from "../i18n";

/** 全文搜索面板：逐页提取文本 → 匹配 → 结果列表；F3/Shift+F3 遍历 */
export default function SearchPanel() {
  const docId = useApp((s) => s.docId);
  const pageCount = useApp((s) => s.pageCount);
  const open = useApp((s) => s.searchOpen);
  const setOpen = useApp((s) => s.setSearchOpen);
  const query = useApp((s) => s.searchQuery);
  const hits = useApp((s) => s.searchHits);
  const active = useApp((s) => s.searchActive);
  const searching = useApp((s) => s.searching);
  const setSearching = useApp((s) => s.setSearching);
  const setSearch = useApp((s) => s.setSearch);
  const setSearchActive = useApp((s) => s.setSearchActive);
  const jumpToPage = useApp((s) => s.jumpToPage);

  const [input, setInput] = useState(query);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const runRef = useRef(0);
  const t = useT();

  useEffect(() => {
    if (open) inputRef.current?.focus();
  }, [open]);

  const runSearch = async () => {
    const q = input.trim();
    if (docId === null || !q) return;
    const run = ++runRef.current;
    setSearching(true);
    setSearch(q, []);
    const collected: SearchHit[] = [];
    const lower = q.toLowerCase();
    // 并发 4 页一批
    for (let start = 0; start < pageCount; start += 4) {
      if (runRef.current !== run) return;
      const batch = await Promise.all(
        Array.from({ length: Math.min(4, pageCount - start) }, (_, i) =>
          getPageText(docId, start + i).catch(() => ""),
        ),
      );
      batch.forEach((text, bi) => {
        const pageIndex = start + bi;
        const lt = text.toLowerCase();
        let pos = lt.indexOf(lower);
        while (pos !== -1 && collected.length < 500) {
          const from = Math.max(0, pos - 20);
          const to = Math.min(text.length, pos + q.length + 30);
          collected.push({
            pageIndex,
            offset: pos,
            snippet: (from > 0 ? "…" : "") + text.slice(from, to).replace(/\s+/g, " ") + (to < text.length ? "…" : ""),
          });
          pos = lt.indexOf(lower, pos + q.length);
        }
      });
      if (runRef.current === run) setSearch(q, [...collected]);
    }
    if (runRef.current === run) {
      setSearching(false);
      if (collected.length === 0) {
        useApp.getState().pushToast("info", t('未找到 "{q}"', { q }));
      }
    }
  };

  const gotoActive = (i: number) => {
    if (!hits.length) return;
    const idx = (i + hits.length) % hits.length;
    setSearchActive(idx);
    jumpToPage(hits[idx].pageIndex, true);
  };

  // F3 / Shift+F3 全局遍历
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F3") {
        e.preventDefault();
        gotoActive(e.shiftKey ? active - 1 : active + 1);
      } else if (e.key === "Escape") {
        setOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, active, hits]);

  // 首个命中自动定位
  useEffect(() => {
    if (hits.length && active === 0) jumpToPage(hits[0].pageIndex, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searching]);

  if (!open || docId === null) return null;

  const q = query.toLowerCase();

  return (
    <div className="search-results">
      <div className="head">
        <span>
          {searching
            ? t("搜索中…")
            : hits.length
              ? `${Math.min(active + 1, hits.length)} / ${hits.length} ${t("个结果")}`
              : t("输入关键词后回车")}
        </span>
        <span>
          <button
            title={t("上一个（Shift+F3）")}
            onClick={() => gotoActive(active - 1)}
            disabled={!hits.length}
          >
            ↑
          </button>
          <button
            title={t("下一个（F3）")}
            onClick={() => gotoActive(active + 1)}
            disabled={!hits.length}
          >
            ↓
          </button>
          <button title={t("关闭")} onClick={() => setOpen(false)}>
            ✕
          </button>
        </span>
      </div>
      <div style={{ padding: "6px 10px", borderBottom: "1px solid var(--border)" }}>
        <input
          ref={inputRef}
          style={{ width: "100%" }}
          placeholder={t("全文搜索，回车确认；F3 遍历结果")}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && runSearch()}
        />
      </div>
      {hits.map((h, i) => (
        <div
          key={i}
          className={`search-hit${i === active ? " active" : ""}`}
          onClick={() => gotoActive(i)}
        >
          <span className="pg">{t("第 {n} 页", { n: h.pageIndex + 1 })}</span>
          <Highlighted text={h.snippet} query={q} />
        </div>
      ))}
    </div>
  );
}

function Highlighted({ text, query }: { text: string; query: string }) {
  if (!query) return <span>{text}</span>;
  const lt = text.toLowerCase();
  const parts: React.ReactNode[] = [];
  let pos = 0;
  let idx = lt.indexOf(query);
  let k = 0;
  while (idx !== -1) {
    if (idx > pos) parts.push(<span key={k++}>{text.slice(pos, idx)}</span>);
    parts.push(<mark key={k++}>{text.slice(idx, idx + query.length)}</mark>);
    pos = idx + query.length;
    idx = lt.indexOf(query, pos);
  }
  if (pos < text.length) parts.push(<span key={k++}>{text.slice(pos)}</span>);
  return <span>{parts}</span>;
}
