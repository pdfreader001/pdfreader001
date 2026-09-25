/**
 * 位图 LRU 缓存：防止大文档内存膨胀。
 * 按字节预算淘汰最久未使用的条目；淘汰时尝试调用 ImageBitmap.close()
 * 释放 GPU 显存，避免频繁切换缩放比时显存累积。
 */

/** 尝试释放可关闭对象的资源（ImageBitmap 等）。失败时静默忽略。 */
function tryClose(v: unknown): void {
  const c = (v as unknown as { close?: () => void }).close;
  if (typeof c === "function") {
    try {
      c.call(v);
    } catch {
      /* ignore */
    }
  }
}

export class LruCache<K, V> {
  private map = new Map<K, V>();
  private bytes: number;
  private sizeOf: (key: K, value: V) => number;

  constructor(maxBytes: number, sizeOf: (key: K, value: V) => number) {
    this.bytes = maxBytes;
    this.sizeOf = sizeOf;
  }

  private current = 0;

  get(key: K): V | undefined {
    const v = this.map.get(key);
    if (v === undefined) return undefined;
    // 触碰 → 移到末尾（最近使用）
    this.map.delete(key);
    this.map.set(key, v);
    return v;
  }

  set(key: K, value: V): void {
    const old = this.map.get(key);
    if (old !== undefined) {
      this.current -= this.sizeOf(key, old);
      this.map.delete(key);
      tryClose(old);
    }
    this.map.set(key, value);
    this.current += this.sizeOf(key, value);
    this.evict();
  }

  delete(key: K): void {
    const old = this.map.get(key);
    if (old !== undefined) {
      this.current -= this.sizeOf(key, old);
      this.map.delete(key);
      tryClose(old);
    }
  }

  clear(): void {
    for (const [, v] of this.map) {
      tryClose(v);
    }
    this.map.clear();
    this.current = 0;
  }

  private evict(): void {
    while (this.current > this.bytes) {
      const oldest = this.map.keys().next().value as K | undefined;
      if (oldest === undefined) break;
      this.delete(oldest);
    }
  }
}

/** 页面位图缓存：约 300MB 预算 */
export const pageCache = new LruCache<string, ImageBitmap>(
  300 * 1024 * 1024,
  (_k, bmp) => bmp.width * bmp.height * 4,
);

/** 缩略图缓存：约 80MB 预算 */
export const thumbCache = new LruCache<string, ImageBitmap>(
  80 * 1024 * 1024,
  (_k, bmp) => bmp.width * bmp.height * 4,
);

export function pageKey(docId: number, pageIndex: number, scale: number): string {
  return `${docId}:${pageIndex}:${scale.toFixed(3)}`;
}

export function thumbKey(docId: number, pageIndex: number): string {
  return `${docId}:t:${pageIndex}`;
}
