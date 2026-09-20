import { invoke } from "@tauri-apps/api/core";

// ---------- 后端数据结构（与 Rust serde camelCase 对应） ----------

export interface PageInfo {
  index: number;
  width: number; // PDF 点（1/72 英寸）
  height: number;
}

export interface DocumentInfo {
  docId: number;
  fileName: string;
  pageCount: number;
  pages: PageInfo[];
}

/** 后端 AppError 序列化后的形状 */
export interface ApiError {
  code: string;
  message: string;
}

export function isApiError(e: unknown): e is ApiError {
  return (
    typeof e === "object" &&
    e !== null &&
    "code" in e &&
    "message" in e &&
    typeof (e as ApiError).message === "string"
  );
}

// ---------- 命令封装 ----------

export function openDocument(path: string, password?: string): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("open_document", { path, password: password ?? null });
}

export function closeDocument(docId: number): Promise<void> {
  return invoke("close_document", { docId });
}

export function saveDocument(docId: number, path?: string): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("save_document", { docId, path: path ?? null });
}

export function undoDocument(docId: number): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("undo_document", { docId });
}

export function canUndo(docId: number): Promise<boolean> {
  return invoke<boolean>("can_undo", { docId });
}

export function getPageText(docId: number, pageIndex: number): Promise<string> {
  return invoke<string>("get_page_text", { docId, pageIndex });
}

// ---------- 二进制渲染 ----------

/**
 * render_page / render_thumbnail 返回 ArrayBuffer：
 * [width:u32LE][height:u32LE][RGBA 像素...]
 * 解析为 ImageBitmap 供 Canvas 直接绘制。
 */
async function renderBinary(cmd: string, args: Record<string, unknown>): Promise<ImageBitmap> {
  const buf = await invoke<ArrayBuffer>(cmd, args);
  const view = new DataView(buf);
  const width = view.getUint32(0, true);
  const height = view.getUint32(4, true);
  const pixels = new Uint8ClampedArray(buf, 8, width * height * 4);
  const imageData = new ImageData(pixels, width, height);
  return createImageBitmap(imageData);
}

export function renderPage(docId: number, pageIndex: number, scale: number): Promise<ImageBitmap> {
  return renderBinary("render_page", { docId, pageIndex, scale });
}

export function renderThumbnail(docId: number, pageIndex: number): Promise<ImageBitmap> {
  return renderBinary("render_thumbnail", { docId, pageIndex });
}
