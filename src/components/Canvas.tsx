import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useApp } from "../state/store";
import { requestPage, usePageCanvas } from "../hooks/useRenderer";
import type { PageInfo } from "../lib/ipc";

const GAP = 16;

/** 单页视图组件：canvas + 页码标签 */
function PageView({
  page,
  pageIndex,
  scale,
  flashNonce,
}: {
  page: PageInfo;
  pageIndex: number;
  scale: number;
  flashNonce: number;
}) {
  const docId = useApp((s) => s.docId);
  const cssW = Math.round(page.width * scale);
  const cssH = Math.round(page.height * scale);
  const { ref } = usePageCanvas(docId, pageIndex, scale, cssW, cssH);
  const [flash, setFlash] = useState(false);
  useEffect(() => {
    if (flashNonce <= 0) return;
    setFlash(true);
    const t = setTimeout(() => setFlash(false), 900);
    return () => clearTimeout(t);
  }, [flashNonce]);

  return (
    <div className={`page-holder${flash ? " flash" : ""}`} style={{ width: cssW, height: cssH }}>
      <canvas ref={ref} style={{ width: cssW, height: cssH }} />
      <span className="page-number-tag">{pageIndex + 1}</span>
    </div>
  );
}

interface Row {
  /** 行内页索引（连续/单页 1 个，双页 2 个） */
  pages: number[];
  top: number;
  height: number;
}

export default function Canvas() {
  const docId = useApp((s) => s.docId);
  const pages = useApp((s) => s.pages);
  const scale = useApp((s) => s.scale);
  const applyFitScale = useApp((s) => s.applyFitScale);
  const setScale = useApp((s) => s.setScale);
  const viewMode = useApp((s) => s.viewMode);
  const fitMode = useApp((s) => s.fitMode);
  const currentPage = useApp((s) => s.currentPage);
  const setCurrentPage = useApp((s) => s.setCurrentPage);
  const jumpTarget = useApp((s) => s.jumpTarget);
  const flashTarget = useApp((s) => s.flashTarget);
  const setScrollTop = useApp((s) => s.setScrollTop);

  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [viewport, setViewport] = useState({ w: 0, h: 0 });
  const [scrollTop, setLocalScroll] = useState(0);

  // 容器尺寸监听
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() =>
      setViewport({ w: el.clientWidth, h: el.clientHeight }),
    );
    ro.observe(el);
    setViewport({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, [docId]);

  // 适应宽度 / 适应页面：自动换算缩放比
  useEffect(() => {
    if (!pages.length || viewport.w === 0) return;
    if (fitMode === "width") {
      let w: number;
      if (viewMode === "dual") {
        const pair = pages[Math.floor(currentPage / 2) * 2];
        const next = pages[Math.floor(currentPage / 2) * 2 + 1];
        w = pair.width + (next ? next.width : 0) + GAP;
      } else {
        w = pages[currentPage].width + GAP;
      }
      applyFitScale(Math.min(8, (viewport.w - 32) / w));
    } else if (fitMode === "page") {
      const p = pages[currentPage];
      const s = Math.min((viewport.w - 32) / p.width, (viewport.h - 32) / p.height);
      applyFitScale(Math.min(8, s));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fitMode, viewport, pages.length, viewMode]);

  // 行布局（单页只渲染当前页；双页成对；连续全部）
  const rows = useMemo<Row[]>(() => {
    if (!pages.length) return [];
    const out: Row[] = [];
    let top = 16;
    if (viewMode === "single") {
      const h = Math.round(pages[currentPage].height * scale);
      out.push({ pages: [currentPage], top: 16, height: h });
      return out;
    }
    if (viewMode === "dual") {
      for (let i = 0; i < pages.length; i += 2) {
        const h = Math.max(
          Math.round(pages[i].height * scale),
          i + 1 < pages.length ? Math.round(pages[i + 1].height * scale) : 0,
        );
        out.push({ pages: [i, i + 1].filter((j) => j < pages.length), top, height: h });
        top += h + GAP;
      }
    } else {
      for (let i = 0; i < pages.length; i++) {
        const h = Math.round(pages[i].height * scale);
        out.push({ pages: [i], top, height: h });
        top += h + GAP;
      }
    }
    return out;
  }, [pages, scale, viewMode, currentPage]);

  const totalHeight = rows.length ? rows[rows.length - 1].top + rows[rows.length - 1].height + 24 : 0;

  // 可视行 + 前后各预渲染 1 行
  const visible = useMemo(() => {
    const first = Math.max(0, rows.findIndex((r) => r.top + r.height >= scrollTop - 200));
    if (first < 0) return { start: 0, end: 0 };
    let end = first;
    while (end < rows.length && rows[end].top <= scrollTop + viewport.h + 200) end++;
    return { start: Math.max(0, first - 1), end: Math.min(rows.length, end + 1) };
  }, [rows, scrollTop, viewport.h]);

  // 滚动 → 当前页 + 位置记忆
  const onScroll = useCallback(() => {
    const el = wrapRef.current;
    if (!el) return;
    const t = el.scrollTop;
    setLocalScroll(t);
    setScrollTop(t);
    const mid = t + el.clientHeight / 2;
    const idx = rows.findIndex((r) => r.top <= mid && r.top + r.height > mid);
    if (idx >= 0) setCurrentPage(rows[idx].pages[0]);
    // 预渲染相邻行
    if (docId !== null) {
      for (let i = Math.max(0, idx - 2); i <= Math.min(rows.length - 1, idx + 2); i++) {
        for (const p of rows[i].pages) requestPage(docId, p, scale).catch(() => {});
      }
    }
  }, [rows, docId, scale, setCurrentPage, setScrollTop]);

  // 跳转（搜索 F3 / 页码输入 / 书签）
  useEffect(() => {
    if (!pages.length) return;
    const el = wrapRef.current;
    if (!el) return;
    const row =
      viewMode === "dual"
        ? rows[Math.floor(jumpTarget.page / 2)]
        : viewMode === "single"
          ? rows[0]
          : rows[jumpTarget.page];
    if (row) el.scrollTo({ top: Math.max(0, row.top - 16), behavior: "smooth" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jumpTarget.nonce]);

  // 打开文档恢复位置
  useEffect(() => {
    if (!pages.length) return;
    const el = wrapRef.current;
    if (!el) return;
    const row =
      viewMode === "dual"
        ? rows[Math.floor(currentPage / 2)]
        : viewMode === "single"
          ? rows[0]
          : rows[currentPage];
    if (row) el.scrollTop = Math.max(0, row.top - 16);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [docId]);

  // Ctrl+滚轮缩放
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
      setScale(scale * factor);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [scale, setScale, docId]);

  if (docId === null || !pages.length) return null;

  return (
    <div className="canvas-wrap" ref={wrapRef} onScroll={onScroll}>
      <div style={{ height: totalHeight, position: "relative" }}>
        {rows.slice(visible.start, visible.end).map((row, i) => {
          const rowIndex = visible.start + i;
          return (
            <div
              key={rowIndex}
              style={{
                position: "absolute",
                top: row.top,
                left: 0,
                right: 0,
                display: "flex",
                justifyContent: "center",
                gap: GAP,
              }}
            >
              {row.pages.map((p) => (
                <PageView
                  key={p}
                  page={pages[p]}
                  pageIndex={p}
                  scale={scale}
                  flashNonce={flashTarget.page === p ? flashTarget.nonce : 0}
                />
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
