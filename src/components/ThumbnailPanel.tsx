import { useEffect, useMemo, useRef, useState } from "react";
import { useApp } from "../state/store";
import { thumbCache, thumbKey } from "../lib/bitmapCache";
import { renderThumbnail } from "../lib/ipc";

const THUMB_W = 150;
const ITEM_PAD = 12;

function ThumbItem({
  docId,
  pageIndex,
  cssH,
  selected,
  current,
  onClick,
}: {
  docId: number;
  pageIndex: number;
  cssH: number;
  selected: boolean;
  current: boolean;
  onClick: (e: React.MouseEvent) => void;
}) {
  const ref = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    let cancelled = false;
    const key = thumbKey(docId, pageIndex);
    const paint = (bmp: ImageBitmap) => {
      const canvas = ref.current;
      if (!canvas || cancelled) return;
      canvas.width = bmp.width;
      canvas.height = bmp.height;
      canvas.getContext("2d")?.drawImage(bmp, 0, 0);
    };
    const cached = thumbCache.get(key);
    if (cached) {
      paint(cached);
      return;
    }
    renderThumbnail(docId, pageIndex)
      .then((bmp) => {
        if (cancelled) return;
        thumbCache.set(key, bmp);
        paint(bmp);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [docId, pageIndex]);

  return (
    <div
      className={`thumb-item${selected ? " selected" : ""}${current ? " current" : ""}`}
      style={{ width: THUMB_W + 20, height: cssH + ITEM_PAD + 22 }}
      onClick={onClick}
    >
      <canvas ref={ref} style={{ width: THUMB_W, height: cssH }} />
      <span className="cap">{pageIndex + 1}</span>
    </div>
  );
}

export default function ThumbnailPanel() {
  const docId = useApp((s) => s.docId);
  const pages = useApp((s) => s.pages);
  const selectedPages = useApp((s) => s.selectedPages);
  const currentPage = useApp((s) => s.currentPage);
  const toggleSelect = useApp((s) => s.toggleSelect);
  const jumpToPage = useApp((s) => s.jumpToPage);

  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(0);

  // 每项高度按页面宽高比推算
  const items = useMemo(() => {
    let top = 0;
    return pages.map((p) => {
      const cssH = (p.height / p.width) * THUMB_W;
      const h = cssH + ITEM_PAD + 22;
      const item = { index: p.index, top, cssH, h };
      top += h;
      return item;
    });
  }, [pages]);
  const totalH = items.length ? items[items.length - 1].top + items[items.length - 1].h : 0;

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    setViewH(el.clientHeight);
    const ro = new ResizeObserver(() => setViewH(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, [docId]);

  // 当前页变化时让缩略图跟随可见
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !items[currentPage]) return;
    const it = items[currentPage];
    if (it.top < el.scrollTop || it.top + it.h > el.scrollTop + el.clientHeight) {
      el.scrollTop = Math.max(0, it.top - 40);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentPage]);

  if (docId === null) return null;

  const overscan = 400;
  const first = Math.max(0, items.findIndex((i) => i.top + i.h >= scrollTop - overscan));
  let last = first;
  while (last < items.length && items[last].top <= scrollTop + viewH + overscan) last++;

  return (
    <div
      className="thumb-list"
      ref={scrollRef}
      onScroll={(e) => setScrollTop((e.target as HTMLDivElement).scrollTop)}
    >
      <div style={{ height: totalH, position: "relative" }}>
        {items.slice(first, last + 1).map((it) => (
          <div
            key={it.index}
            style={{ position: "absolute", top: it.top, left: 0, right: 0, display: "flex", justifyContent: "center" }}
          >
            <ThumbItem
              docId={docId}
              pageIndex={it.index}
              cssH={it.cssH}
              selected={selectedPages.has(it.index)}
              current={it.index === currentPage}
              onClick={(e) => {
                toggleSelect(it.index, e.ctrlKey || e.metaKey, e.shiftKey);
                if (!e.ctrlKey && !e.metaKey && !e.shiftKey) jumpToPage(it.index);
              }}
            />
          </div>
        ))}
      </div>
    </div>
  );
}
