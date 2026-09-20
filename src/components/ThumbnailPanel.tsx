import { useEffect, useMemo, useRef, useState } from "react";
import { useApp } from "../state/store";
import { thumbCache, thumbKey } from "../lib/bitmapCache";
import {
  renderThumbnail,
  canUndo,
  canRedo,
  rotatePages,
  deletePages,
  duplicatePages,
  insertBlankPage,
  reorderPages,
} from "../lib/ipc";
import { useT } from "../i18n";

const THUMB_W = 150;
const ITEM_PAD = 12;

interface ContextMenuState {
  x: number;
  y: number;
  pageIndex: number;
}

function ThumbItem({
  docId,
  pageIndex,
  cssH,
  selected,
  current,
  isDropTarget,
  dropPosition,
  onClick,
  onContextMenu,
  onDragStart,
  onDragOver,
  onDrop,
  onDragEnd,
}: {
  docId: number;
  pageIndex: number;
  cssH: number;
  selected: boolean;
  current: boolean;
  isDropTarget: boolean;
  dropPosition: "before" | "after" | null;
  onClick: (e: React.MouseEvent) => void;
  onContextMenu: (e: React.MouseEvent) => void;
  onDragStart: (e: React.DragEvent) => void;
  onDragOver: (e: React.DragEvent) => void;
  onDrop: (e: React.DragEvent) => void;
  onDragEnd: () => void;
}) {
  const ref = useRef<HTMLCanvasElement | null>(null);
  const renderRevision = useApp((s) => s.renderRevision);

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
  }, [docId, pageIndex, renderRevision]);

  return (
    <div
      className={`thumb-item${selected ? " selected" : ""}${current ? " current" : ""}${
        isDropTarget && dropPosition === "before" ? " drop-before" : ""
      }${isDropTarget && dropPosition === "after" ? " drop-after" : ""}`}
      style={{ width: THUMB_W + 20, height: cssH + ITEM_PAD + 22 }}
      onClick={onClick}
      onContextMenu={onContextMenu}
      draggable
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDrop={onDrop}
      onDragEnd={onDragEnd}
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
  const updatePages = useApp((s) => s.updatePages);
  const setCanUndo = useApp((s) => s.setCanUndo);
  const setCanRedo = useApp((s) => s.setCanRedo);
  const markDirty = useApp((s) => s.markDirty);
  const errorToast = useApp((s) => s.errorToast);
  const pushToast = useApp((s) => s.pushToast);
  const clearSelection = useApp((s) => s.clearSelection);
  const t = useT();

  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(0);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);
  const [dropPosition, setDropPosition] = useState<"before" | "after" | null>(null);
  const [dragging, setDragging] = useState(false);

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

  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !items[currentPage]) return;
    const it = items[currentPage];
    if (it.top < el.scrollTop || it.top + it.h > el.scrollTop + el.clientHeight) {
      el.scrollTop = Math.max(0, it.top - 40);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentPage]);

  useEffect(() => {
    const onDocClick = () => setContextMenu(null);
    document.addEventListener("click", onDocClick);
    return () => document.removeEventListener("click", onDocClick);
  }, []);

  const handleRotate = async (delta: number) => {
    if (docId === null || !contextMenu) return;
    const pages = selectedPages.size > 0 ? Array.from(selectedPages) : [contextMenu.pageIndex];
    setContextMenu(null);
    try {
      const info = await rotatePages(docId, pages, delta);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId)); setCanRedo(await canRedo(docId));
      pushToast("info", t("已旋转 {n} 页", { n: pages.length }));
    } catch (e) {
      errorToast(e);
    }
  };

  const handleDelete = async () => {
    if (docId === null || !contextMenu) return;
    const pages = selectedPages.size > 0 ? Array.from(selectedPages) : [contextMenu.pageIndex];
    setContextMenu(null);
    if (!confirm(t("确定删除 {n} 页？此操作可撤销。", { n: pages.length }))) return;
    try {
      const info = await deletePages(docId, pages);
      updatePages(info);
      markDirty(true);
      clearSelection();
      setCanUndo(await canUndo(docId)); setCanRedo(await canRedo(docId));
      pushToast("info", t("已删除 {n} 页", { n: pages.length }));
    } catch (e) {
      errorToast(e);
    }
  };

  const handleDuplicate = async () => {
    if (docId === null || !contextMenu) return;
    const pages = selectedPages.size > 0 ? Array.from(selectedPages) : [contextMenu.pageIndex];
    const dest = contextMenu.pageIndex + 1;
    setContextMenu(null);
    try {
      const info = await duplicatePages(docId, pages, dest);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId)); setCanRedo(await canRedo(docId));
      pushToast("info", t("已复制 {n} 页", { n: pages.length }));
    } catch (e) {
      errorToast(e);
    }
  };

  const handleInsertBlank = async () => {
    if (docId === null || !contextMenu) return;
    const atIndex = contextMenu.pageIndex;
    const refPage = pages[contextMenu.pageIndex] || pages[0];
    const width = refPage?.width || 595;
    const height = refPage?.height || 842;
    setContextMenu(null);
    try {
      const info = await insertBlankPage(docId, atIndex, width, height);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId)); setCanRedo(await canRedo(docId));
      pushToast("info", t("已插入空白页"));
    } catch (e) {
      errorToast(e);
    }
  };

  const handleDragStart = (e: React.DragEvent, pageIndex: number) => {
    if (docId === null) return;
    setDragging(true);
    const indices =
      selectedPages.size > 0 && selectedPages.has(pageIndex)
        ? Array.from(selectedPages)
        : [pageIndex];
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", JSON.stringify(indices));
  };

  const handleDragOver = (e: React.DragEvent, pageIndex: number) => {
    if (!dragging) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const midY = rect.top + rect.height / 2;
    const pos = e.clientY < midY ? "before" : "after";
    setDragOverIndex(pageIndex);
    setDropPosition(pos);
  };

  const handleDrop = async (e: React.DragEvent, targetIndex: number) => {
    if (docId === null || !dragging) return;
    e.preventDefault();
    const data = e.dataTransfer.getData("text/plain");
    let indices: number[] = [];
    try {
      indices = JSON.parse(data);
    } catch {
      return;
    }
    const pos = dropPosition || "after";
    const toIndex = pos === "before" ? targetIndex : targetIndex + 1;
    setDragOverIndex(null);
    setDropPosition(null);
    setDragging(false);
    if (indices.length === 0) return;
    const sorted = [...indices].sort((a, b) => a - b);
    const allInRange = sorted.every((i) => i >= 0 && i < pages.length);
    if (!allInRange) return;
    const isTrivial = sorted.length === 1 && sorted[0] === toIndex;
    if (isTrivial) return;
    try {
      const info = await reorderPages(docId, indices, toIndex);
      updatePages(info);
      markDirty(true);
      setCanUndo(await canUndo(docId)); setCanRedo(await canRedo(docId));
      pushToast("info", t("已移动 {n} 页", { n: indices.length }));
    } catch (err) {
      errorToast(err);
    }
  };

  const handleDragEnd = () => {
    setDragging(false);
    setDragOverIndex(null);
    setDropPosition(null);
  };

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
            style={{
              position: "absolute",
              top: it.top,
              left: 0,
              right: 0,
              display: "flex",
              justifyContent: "center",
            }}
          >
            <ThumbItem
              docId={docId}
              pageIndex={it.index}
              cssH={it.cssH}
              selected={selectedPages.has(it.index)}
              current={it.index === currentPage}
              isDropTarget={dragOverIndex === it.index}
              dropPosition={dragOverIndex === it.index ? dropPosition : null}
              onClick={(e) => {
                toggleSelect(it.index, e.ctrlKey || e.metaKey, e.shiftKey);
                if (!e.ctrlKey && !e.metaKey && !e.shiftKey) jumpToPage(it.index);
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                setContextMenu({ x: e.clientX, y: e.clientY, pageIndex: it.index });
              }}
              onDragStart={(e) => handleDragStart(e, it.index)}
              onDragOver={(e) => handleDragOver(e, it.index)}
              onDrop={(e) => handleDrop(e, it.index)}
              onDragEnd={handleDragEnd}
            />
          </div>
        ))}
      </div>
      {contextMenu && (
        <div
          className="context-menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <button onClick={() => handleRotate(-90)}>{t("↺ 逆时针旋转 90°")}</button>
          <button onClick={() => handleRotate(90)}>{t("↻ 顺时针旋转 90°")}</button>
          <button onClick={() => handleRotate(180)}>{t("⟲ 旋转 180°")}</button>
          <div className="ctx-sep" />
          <button onClick={handleDuplicate}>{t("📋 复制页面")}</button>
          <button onClick={handleInsertBlank}>{t("➕ 插入空白页")}</button>
          <button onClick={handleDelete} style={{ color: "var(--danger)" }}>
            {t("🗑 删除页面")}
          </button>
          <div className="ctx-sep" />
          <button onClick={() => setContextMenu(null)}>{t("📤 提取为新文档")}</button>
        </div>
      )}
    </div>
  );
}


