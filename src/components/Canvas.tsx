import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useApp } from "../state/store";
import { requestPage, usePageCanvas } from "../hooks/useRenderer";
import type { PageInfo, AnnotationInfo } from "../lib/ipc";
import {
  searchPageText,
  pickTextAtPoint,
  deleteAnnotation,
  listAnnotations,
  refreshUndoRedo,
} from "../lib/ipc";
import type { SearchHitRect } from "../lib/ipc";
import { useT } from "../i18n";
import { factorsFromBox } from "../lib/watermarkLayout";
import type { DeepEditMode, PdfRegion } from "../state/store";

const GAP = 16;

/** 单页视图组件：canvas + 页码标签 */
const PageView = React.memo(function PageView({
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

  // 选区交互：仅当 selectingFor 非空时启用
  const selectingFor = useApp((s) => s.selectingFor);
  // 精细订阅：只有本 PageView 的 liveSelection 才订阅，避免拖拽时所有 PageView 重渲
  const liveSelection = useApp((s) =>
    s.liveSelection && s.liveSelection.pageIndex === pageIndex ? s.liveSelection : null,
  );
  const setLiveSelection = useApp((s) => s.setLiveSelection);
  const setCompletedSelection = useApp((s) => s.setCompletedSelection);
  const setCurrentPage = useApp((s) => s.setCurrentPage);
  // 精细订阅：只有本页处于文字编辑态时才渲染虚线框
  const editTarget = useApp((s) =>
    s.editTarget && s.editTarget.pageIndex === pageIndex ? s.editTarget : null,
  );
  const holderRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<{ x: number; y: number } | null>(null);
  /** 水印拖放：记录抓取偏移与对象高度（CSS 像素），松手后清空 */
  const wmDragRef = useRef<{ grabX: number; grabY: number; boxH: number } | null>(null);

  // 搜索高亮：读取当前页的 hits
  const searchQuery = useApp((s) => s.searchQuery);
  // 精细订阅：只订阅当前页的 hits，避免其它页变化时本 PageView 重渲
  const pageHits = useApp((s) => s.searchHighlights[pageIndex]);
  const setPageHighlights = useApp((s) => s.setPageHighlights);
  const addLoadingHighlight = useApp((s) => s.addLoadingHighlight);
  const removeLoadingHighlight = useApp((s) => s.removeLoadingHighlight);
  const searchActive = useApp((s) => s.searchActive);
  const searchHits = useApp((s) => s.searchHits);

  // 注释：精细订阅当前页的注释列表
  const pageAnnotations: AnnotationInfo[] = useApp((s) => s.annotations[pageIndex] ?? []);
  const setPageAnnotations = useApp((s) => s.setPageAnnotations);
  // 水印去除预览区域：所有页都叠加显示（水印按位置在各页重复出现）
  const removalPreview = useApp((s) => s.removalPreview);
  // 水印添加预览：只叠加在当前页（由 WatermarkAddPanel 写入）
  const watermarkPreview = useApp((s) => s.watermarkPreview);
  const setWatermarkCustomPos = useApp((s) => s.setWatermarkCustomPos);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const setUndoRedo = useApp((s) => s.setUndoRedo);
  const refreshAnnotations = useCallback(async () => {
    if (docId === null) return;
    try {
      const list = await listAnnotations(docId, pageIndex);
      setPageAnnotations(pageIndex, list);
    } catch {
      /* 刷新失败静默 */
    }
  }, [docId, pageIndex, setPageAnnotations]);

  // 右键菜单状态
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    annotation: AnnotationInfo;
  } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const t = useT();

  const onAnnotationContextMenu = useCallback((e: React.MouseEvent, ann: AnnotationInfo) => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenu({ x: e.clientX, y: e.clientY, annotation: ann });
  }, []);

  const handleDeleteAnnotation = useCallback(async () => {
    if (!contextMenu || docId === null) return;
    const ann = contextMenu.annotation;
    setDeleting(true);
    try {
      const info = await deleteAnnotation(docId, ann.pageIndex, ann.index);
      updatePages(info);
      markDirty(true);
      setUndoRedo(await refreshUndoRedo(docId));
      // 局部刷新当前页注释
      await refreshAnnotations();
      setContextMenu(null);
    } catch (e) {
      useApp.getState().errorToast(e);
    } finally {
      setDeleting(false);
    }
  }, [contextMenu, docId, updatePages, markDirty, setUndoRedo, refreshAnnotations]);

  // 点击空白处关闭右键菜单
  useEffect(() => {
    if (!contextMenu) return;
    const onDown = () => setContextMenu(null);
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [contextMenu]);

  // 当前页命中高亮矩形（PDF 点），由精细订阅直接得到（可能为 undefined）
  const pageHitRects: SearchHitRect[] = pageHits ?? [];

  // 当前命中是全局第几个 → 当前页内第几个
  const activeIndexOnPage = useMemo(() => {
    if (searchActive < 0 || !searchHits.length) return -1;
    const activeHit = searchHits[searchActive];
    if (!activeHit || activeHit.pageIndex !== pageIndex) return -1;
    // 统计当前页中，offset 小于等于 activeHit.offset 的命中数量 - 1
    // （按顺序数到当前命中是第几个）
    let idx = 0;
    for (const h of searchHits) {
      if (h.pageIndex !== pageIndex) continue;
      if (h.offset === activeHit.offset) return idx;
      idx++;
    }
    return -1;
  }, [searchActive, searchHits, pageIndex]);

  // 有搜索词且当前页无高亮 → 拉取
  // 注意：searchHighlights / loadingHighlights 都通过 useApp.getState() 读取，
  // 避免它们加入依赖触发所有 PageView 的 useEffect 重跑（500 页文档时巨大开销）。
  useEffect(() => {
    if (!searchQuery || docId === null) return;
    const state = useApp.getState();
    if (state.searchHighlights[pageIndex]) return;
    if (state.loadingHighlights.has(pageIndex)) return;
    addLoadingHighlight(pageIndex);
    searchPageText(docId, pageIndex, searchQuery, 100)
      .then((res) => {
        setPageHighlights(pageIndex, res.hits);
      })
      .catch(() => {
        // 失败也标记为空，避免重复请求
        setPageHighlights(pageIndex, []);
      })
      .finally(() => {
        removeLoadingHighlight(pageIndex);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchQuery, docId, pageIndex]);

  const onMouseDown = (e: React.MouseEvent) => {
    if (!selectingFor) return;
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const rect = holderRef.current?.getBoundingClientRect();
    if (!rect) return;
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    setCurrentPage(pageIndex);
    // 点击模式（addText）：mousedown 即完成选区
    if (selectingFor === "addText") {
      setCompletedSelection({
        pageIndex,
        rect: { left: x, top: y, right: x, bottom: y },
        scale,
      });
      return;
    }
    // 拖拽模式（rewrite / watermarkRemove）：记录起点，等 mouseup 完成
    dragRef.current = { x, y };
    setLiveSelection({ left: x, top: y, right: x, bottom: y, pageIndex });
  };
  const onMouseMove = (e: React.MouseEvent) => {
    if (!dragRef.current || !holderRef.current) return;
    const rect = holderRef.current.getBoundingClientRect();
    const x = Math.max(0, Math.min(cssW, e.clientX - rect.left));
    const y = Math.max(0, Math.min(cssH, e.clientY - rect.top));
    const start = dragRef.current;
    setLiveSelection({
      left: Math.min(start.x, x),
      top: Math.min(start.y, y),
      right: Math.max(start.x, x),
      bottom: Math.max(start.y, y),
      pageIndex,
    });
  };
  const onMouseUp = () => {
    if (!dragRef.current) return;
    dragRef.current = null;
    const live = useApp.getState().liveSelection;
    if (!live || live.right - live.left < 4 || live.bottom - live.top < 4) {
      setLiveSelection(null);
      return;
    }
    // 复制去掉 pageIndex 字段后传给 setCompletedSelection
    const { pageIndex: _ignore, ...rect } = live;
    setCompletedSelection({
      pageIndex,
      rect,
      scale,
    });
  };

  // 水印拖放：抓取对象并反解为归一化位置写回 store（与后端 custom 语义一致）
  const onWmHandleDown = (e: React.PointerEvent, r: PdfRegion) => {
    e.preventDefault();
    e.stopPropagation();
    const rect = holderRef.current?.getBoundingClientRect();
    if (!rect) return;
    const left = r.left * scale;
    const top = (page.height - r.top) * scale;
    wmDragRef.current = {
      grabX: e.clientX - rect.left - left,
      grabY: e.clientY - rect.top - top,
      boxH: (r.top - r.bottom) * scale,
    };
    setCurrentPage(pageIndex);
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const onWmHandleMove = (e: React.PointerEvent, r: PdfRegion) => {
    const drag = wmDragRef.current;
    const rect = holderRef.current?.getBoundingClientRect();
    if (!drag || !rect) return;
    e.preventDefault();
    e.stopPropagation();
    const newLeft = e.clientX - rect.left - drag.grabX;
    const newTop = e.clientY - rect.top - drag.grabY;
    // 反解为归一化因子；factorsFromBox 内部钳制，天然把对象限制在页面内
    setWatermarkCustomPos(
      factorsFromBox(
        newLeft,
        newTop + drag.boxH,
        scale,
        page.width,
        page.height,
        r.right - r.left,
        r.top - r.bottom,
      ),
    );
  };
  const onWmHandleUp = (e: React.PointerEvent) => {
    if (!wmDragRef.current) return;
    wmDragRef.current = null;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  };

  // 双击文字：调用后端定位到词/词组 → 打开重写 modal
  const onDoubleClick = useCallback(
    async (e: React.MouseEvent) => {
      if (selectingFor) return; // 选区模式下不触发
      if (docId === null) return;
      // 仅在内容编辑态取字：否则会在画布留下不消失的虚线框，
      // 且 dblClickText 残留会在之后打开面板时意外弹出重写弹窗。
      const st = useApp.getState();
      if (st.task !== "edit") return;
      const rect = holderRef.current?.getBoundingClientRect();
      if (!rect) return;
      const cssX = e.clientX - rect.left;
      const cssY = e.clientY - rect.top;
      if (scale <= 0) return;
      // 转 PDF 点（左下原点）
      const x = cssX / scale;
      const y = (cssH - cssY) / scale;
      try {
        const result = await pickTextAtPoint(docId, pageIndex, x, y);
        if (!result) return;
        const region = {
          left: result.left,
          bottom: result.bottom,
          right: result.right,
          top: result.top,
        };
        // 进入文字编辑态：先在画布上标出虚线框 + 光标，再由 EditPanel 打开重写面板。
        st.setEditTab("deep");
        st.setEditTarget({ pageIndex, region });
        st.setDblClickText({
          region,
          pageIndex,
          originalText: result.text,
          fontName: result.fontName,
          fontSize: result.fontSize,
          color: result.color,
        });
      } catch {
        // 双击命中失败静默忽略
      }
    },
    [docId, pageIndex, scale, selectingFor, cssH],
  );

  return (
    <div
      ref={holderRef}
      className={`page-holder${flash ? " flash" : ""}${
        selectingFor ? " selecting" : ""
      }${selectingFor === "addText" ? " selecting-point" : ""}`}
      style={{ width: cssW, height: cssH }}
      onMouseDown={onMouseDown}
      onMouseMove={onMouseMove}
      onMouseUp={onMouseUp}
      onMouseLeave={onMouseUp}
      onDoubleClick={onDoubleClick}
    >
      <canvas ref={ref} style={{ width: cssW, height: cssH }} />
      <span className="page-number-tag">{pageIndex + 1}</span>
      {/* 文字编辑态：双击命中的区域显示虚线框 + 闪烁光标 */}
      {editTarget &&
        (() => {
          const r = editTarget.region;
          const left = r.left * scale;
          const top = (page.height - r.top) * scale;
          const width = (r.right - r.left) * scale;
          const height = (r.top - r.bottom) * scale;
          if (width <= 0 || height <= 0) return null;
          return (
            <div className="text-edit-region" style={{ left, top, width, height }}>
              <span className="text-edit-caret" />
            </div>
          );
        })()}
      {/* 搜索高亮 overlay */}
      {pageHitRects.length > 0 &&
        pageHitRects.map((r, i) => {
          // PDF 点（左下） → CSS 像素（左上）
          const left = r.left * scale;
          const top = (page.height - r.top) * scale;
          const width = (r.right - r.left) * scale;
          const height = (r.top - r.bottom) * scale;
          if (width <= 0 || height <= 0) return null;
          const isActive = i === activeIndexOnPage;
          return (
            <div
              key={`sh-${i}`}
              className={`search-highlight${isActive ? " active" : ""}`}
              style={{
                left,
                top,
                width,
                height,
              }}
            />
          );
        })}
      {/* 注释 overlay */}
      {pageAnnotations.length > 0 &&
        pageAnnotations.map((ann) => {
          const left = ann.left * scale;
          const top = (page.height - ann.top) * scale;
          const width = (ann.right - ann.left) * scale;
          const height = (ann.top - ann.bottom) * scale;
          if (width <= 0 || height <= 0) return null;
          const kind = ann.kind;
          const color = ann.color || "#ffeb3b";

          let className = "annot-overlay";
          const style: React.CSSProperties = {
            left,
            top,
            width,
            height,
            borderColor: color,
            background: "transparent",
          };

          if (kind === "highlight") {
            className += " annot-highlight";
            style.background = color;
            style.opacity = 0.35;
          } else if (kind === "underline") {
            className += " annot-underline";
            style.borderBottom = `2px solid ${color}`;
          } else if (kind === "strikeout") {
            className += " annot-strikeout";
            style.background = `linear-gradient(transparent ${height / 2 - 1}px, ${color} ${height / 2 - 1}px, ${color} ${height / 2 + 1}px, transparent ${height / 2 + 1}px)`;
          } else if (kind === "square") {
            className += " annot-square";
            style.border = `2px solid ${color}`;
          } else if (kind === "freeText") {
            className += " annot-freetext";
            style.border = `1px dashed ${color}`;
            style.background = `${color}10`;
          } else if (kind === "stickyNote") {
            className += " annot-stickynote";
            style.border = `1px solid ${color}`;
            style.background = `${color}30`;
            style.width = Math.max(20, width);
            style.height = Math.max(20, height);
          }

          return (
            <div
              key={`ann-${ann.index}`}
              className={className}
              style={style}
              onContextMenu={(e) => onAnnotationContextMenu(e, ann)}
              title={ann.contents || kind}
            />
          );
        })}
      {/* 水印去除预览：标出「将被删除」的区域（强制预览确认步骤）。
          pageIndex 为 null 表示该预览作用于所有页（自动检测命中的是每页同位置重复对象）。 */}
      {removalPreview &&
        (removalPreview.pageIndex === null || removalPreview.pageIndex === pageIndex) &&
        removalPreview.regions.map((r, i) => {
          const left = r.left * scale;
          const top = (page.height - r.top) * scale;
          const width = (r.right - r.left) * scale;
          const height = (r.top - r.bottom) * scale;
          if (width <= 0 || height <= 0) return null;
          return (
            <div
              key={`rmp-${i}`}
              className="removal-preview-rect"
              style={{ left, top, width, height }}
            />
          );
        })}
      {/* 水印添加预览：把「将要写入」的水印按后端几何叠加在当前页（实时所见即所得）。
          旋转语义与 PDFium 一致（绕对象左下角顺时针），故 transform-origin 取 left bottom。 */}
      {watermarkPreview &&
        watermarkPreview.pageIndex === pageIndex &&
        watermarkPreview.kind === "text" &&
        watermarkPreview.text !== "" &&
        watermarkPreview.regions.map((r, i) => {
          const width = (r.right - r.left) * scale;
          const height = (r.top - r.bottom) * scale;
          if (width <= 0 || height <= 0) return null;
          return (
            <div
              key={`wmp-${i}`}
              className="wm-preview-text"
              style={{
                left: r.left * scale,
                top: (page.height - r.top) * scale,
                fontSize: watermarkPreview.fontSize * scale,
                color: watermarkPreview.color,
                opacity: watermarkPreview.opacity / 100,
                transform: `rotate(${watermarkPreview.rotation}deg)`,
              }}
            >
              {watermarkPreview.text}
            </div>
          );
        })}
      {watermarkPreview &&
        watermarkPreview.pageIndex === pageIndex &&
        watermarkPreview.kind === "image" &&
        watermarkPreview.imageSrc !== null &&
        watermarkPreview.regions.map((r, i) => {
          const width = (r.right - r.left) * scale;
          const height = (r.top - r.bottom) * scale;
          if (width <= 0 || height <= 0) return null;
          return (
            <img
              key={`wmp-${i}`}
              className="wm-preview-image"
              src={watermarkPreview.imageSrc ?? undefined}
              alt=""
              style={{
                left: r.left * scale,
                top: (page.height - r.top) * scale,
                width,
                height,
                opacity: watermarkPreview.opacity / 100,
                transform: `rotate(${watermarkPreview.rotation}deg)`,
              }}
            />
          );
        })}
      {/* 水印拖放手柄：单点（非平铺）时覆盖在对象上，拖动即改位置（写 watermarkCustomPos） */}
      {watermarkPreview &&
        watermarkPreview.pageIndex === pageIndex &&
        !watermarkPreview.tiled &&
        watermarkPreview.regions.length === 1 &&
        (() => {
          const r = watermarkPreview.regions[0];
          const left = r.left * scale;
          const top = (page.height - r.top) * scale;
          const width = (r.right - r.left) * scale;
          const height = (r.top - r.bottom) * scale;
          if (width <= 0 || height <= 0) return null;
          return (
            <div
              className="wm-drag-handle"
              style={{ left, top, width, height }}
              title={t("拖动调整位置")}
              onPointerDown={(e) => onWmHandleDown(e, r)}
              onPointerMove={(e) => onWmHandleMove(e, r)}
              onPointerUp={onWmHandleUp}
              onPointerCancel={onWmHandleUp}
            />
          );
        })()}
      {/* 注释右键菜单 */}
      {contextMenu && (
        <div
          className="annot-context-menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <button onClick={handleDeleteAnnotation} disabled={deleting}>
            {deleting ? t("删除中…") : t("删除注释")}
          </button>
        </div>
      )}
      {selectingFor &&
        liveSelection &&
        liveSelection.right - liveSelection.left > 1 &&
        liveSelection.bottom - liveSelection.top > 1 && (
          <div
            className="selection-rect"
            style={{
              left: liveSelection.left,
              top: liveSelection.top,
              width: liveSelection.right - liveSelection.left,
              height: liveSelection.bottom - liveSelection.top,
            }}
          />
        )}
    </div>
  );
});

interface Row {
  /** 行内页索引（连续/单页 1 个，双页 2 个） */
  pages: number[];
  top: number;
  height: number;
}

/** 编辑态浮动工具胶囊：仅当 task === "edit" 时显示在画布底部居中。 */
function EditToolCapsule() {
  const t = useT();
  const editMode = useApp((s) => s.editMode);
  const setEditMode = useApp((s) => s.setEditMode);
  const setEditTab = useApp((s) => s.setEditTab);
  const tools: { id: DeepEditMode; label: string }[] = [
    { id: "select", label: t("选择") },
    { id: "rewrite", label: t("编辑") },
    { id: "addtext", label: t("文字") },
  ];
  return (
    <div className="edit-capsule">
      {tools.map((tool) => (
        <button
          key={tool.id}
          type="button"
          className={editMode === tool.id ? "active" : ""}
          onClick={() => {
            // 面板默认停在「注释」页签，不切页签会出现「高亮但画布无反应」。
            setEditTab("deep");
            // 不做 toggle-off：框选完成后 selectingFor 会被清空，
            // 再次点击已激活的工具正好是「重新武装选区」的通道。
            setEditMode(tool.id);
          }}
        >
          {tool.label}
        </button>
      ))}
    </div>
  );
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
  const task = useApp((s) => s.task);

  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [viewport, setViewport] = useState({ w: 0, h: 0 });
  const [scrollTop, setLocalScroll] = useState(0);
  // rAF throttle：滚动事件用 ref 合并，避免每帧都触发 React 重渲
  const scrollRafRef = useRef<number | null>(null);
  const pendingScrollTop = useRef<number>(0);

  // 容器尺寸监听
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewport({ w: el.clientWidth, h: el.clientHeight }));
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

  const totalHeight = rows.length
    ? rows[rows.length - 1].top + rows[rows.length - 1].height + 24
    : 0;

  // 可视行 + 前后各预渲染 1 行
  const visible = useMemo(() => {
    const first = Math.max(
      0,
      rows.findIndex((r) => r.top + r.height >= scrollTop - 200),
    );
    if (first < 0) return { start: 0, end: 0 };
    let end = first;
    while (end < rows.length && rows[end].top <= scrollTop + viewport.h + 200) end++;
    return { start: Math.max(0, first - 1), end: Math.min(rows.length, end + 1) };
  }, [rows, scrollTop, viewport.h]);

  // 滚动 → 当前页 + 位置记忆（rAF 节流，避免高频重渲）
  const onScroll = useCallback(() => {
    const el = wrapRef.current;
    if (!el) return;
    const t = el.scrollTop;
    pendingScrollTop.current = t;
    setScrollTop(t); // store 更新保持原样（轻量）
    if (scrollRafRef.current !== null) return;
    scrollRafRef.current = requestAnimationFrame(() => {
      scrollRafRef.current = null;
      setLocalScroll(pendingScrollTop.current);
      const st = pendingScrollTop.current;
      const mid = st + el.clientHeight / 2;
      const idx = rows.findIndex((r) => r.top <= mid && r.top + r.height > mid);
      if (idx >= 0) setCurrentPage(rows[idx].pages[0]);
      // 预渲染相邻行
      if (docId !== null) {
        for (let i = Math.max(0, idx - 2); i <= Math.min(rows.length - 1, idx + 2); i++) {
          for (const p of rows[i].pages) requestPage(docId, p, scale).catch(() => {});
        }
      }
    });
  }, [rows, docId, scale, setCurrentPage, setScrollTop]);

  // 卸载时取消未完成的 rAF
  useEffect(() => {
    return () => {
      if (scrollRafRef.current !== null) {
        cancelAnimationFrame(scrollRafRef.current);
      }
    };
  }, []);

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
    <div className="canvas-stage">
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
      {task === "edit" && <EditToolCapsule />}
    </div>
  );
}
