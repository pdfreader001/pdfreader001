import { useEffect, useRef, useState } from "react";
import { renderPage } from "../lib/ipc";
import { pageCache, pageKey } from "../lib/bitmapCache";
import { useApp } from "../state/store";

/**
 * 渲染单页位图到 ImageBitmap。
 * - 命中 LRU 缓存直接返回
 * - 同一 key 并发请求合并（in-flight 去重）
 * - 请求过期（页码/缩放已变化）时丢弃结果
 */
const inflight = new Map<string, Promise<ImageBitmap>>();

export function requestPage(docId: number, pageIndex: number, scale: number): Promise<ImageBitmap> {
  const key = pageKey(docId, pageIndex, scale);
  const cached = pageCache.get(key);
  if (cached) return Promise.resolve(cached);
  let p = inflight.get(key);
  if (!p) {
    p = renderPage(docId, pageIndex, scale)
      .then((bmp) => {
        pageCache.set(key, bmp);
        return bmp;
      })
      .finally(() => inflight.delete(key));
    inflight.set(key, p);
  }
  return p;
}

/**
 * 把指定位图绘制到 canvas 上（按 devicePixelRatio 适配）。
 */
export function usePageCanvas(
  docId: number | null,
  pageIndex: number,
  scale: number,
  cssWidth: number,
  cssHeight: number,
) {
  const ref = useRef<HTMLCanvasElement | null>(null);
  const [ready, setReady] = useState(false);
  const renderRevision = useApp((s) => s.renderRevision);

  useEffect(() => {
    if (docId === null) return;
    let cancelled = false;
    setReady(false);
    requestPage(docId, pageIndex, scale).then((bmp) => {
      if (cancelled) return;
      const canvas = ref.current;
      if (!canvas) return;
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.round(cssWidth * dpr);
      canvas.height = Math.round(cssHeight * dpr);
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.drawImage(bmp, 0, 0, canvas.width, canvas.height);
      setReady(true);
    });
    return () => {
      cancelled = true;
    };
  }, [docId, pageIndex, scale, cssWidth, cssHeight, renderRevision]);

  return { ref, ready };
}
