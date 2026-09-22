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
  args?: Record<string, string>;
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

export function redoDocument(docId: number): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("redo_document", { docId });
}

export function canRedo(docId: number): Promise<boolean> {
  return invoke<boolean>("can_redo", { docId });
}

export function getPageText(docId: number, pageIndex: number): Promise<string> {
  return invoke<string>("get_page_text", { docId, pageIndex });
}

export interface BookmarkNode {
  title: string;
  pageIndex: number;
  level: number;
  children: BookmarkNode[];
}

export interface MergeSource {
  path: string;
  ranges?: string | null;
}

export type SplitMode =
  | { mode: "every_n"; payload: { n: number } }
  | { mode: "ranges"; payload: { ranges: string } }
  | { mode: "by_bookmark"; payload: { level?: number | null } }
  | { mode: "selected"; payload: { pages: number[] } };

export function rotatePages(
  docId: number,
  pages: number[],
  deltaDeg: number,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("rotate_pages", { docId, pages, deltaDeg });
}

export function deletePages(docId: number, pages: number[]): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("delete_pages", { docId, pages });
}

export function duplicatePages(
  docId: number,
  pages: number[],
  destIndex: number,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("duplicate_pages", { docId, pages, destIndex });
}

export function insertBlankPage(
  docId: number,
  atIndex: number,
  width: number,
  height: number,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("insert_blank_page", { docId, atIndex, width, height });
}

export function reorderPages(
  docId: number,
  fromIndices: number[],
  toIndex: number,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("reorder_pages", { docId, fromIndices, toIndex });
}

export function extractPages(
  docId: number,
  pages: number[],
  outputPath?: string | null,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("extract_pages", {
    docId,
    pages,
    outputPath: outputPath ?? null,
  });
}

export function mergeDocuments(
  sources: MergeSource[],
  outputPath?: string | null,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("merge_documents", {
    sources,
    outputPath: outputPath ?? null,
  });
}

export function splitDocument(
  docId: number,
  mode: SplitMode,
  outputDir: string,
): Promise<string[]> {
  return invoke<string[]>("split_document", { docId, mode, outputDir });
}

export function getBookmarks(docId: number): Promise<BookmarkNode[]> {
  return invoke<BookmarkNode[]>("get_bookmarks", { docId });
}

// ---------- 水印（M4） ----------

export interface WatermarkStyle {
  /** 0–100 */
  opacity: number;
  /** 顺时针角度 */
  rotation: number;
  /** 九宫格：top-left … center … bottom-right */
  position: string;
  tiled: boolean;
  /** 平铺间距（pt） */
  tileSpacing: number;
}

export interface TextWatermarkOpts {
  text: string;
  fontSize: number;
  /** "#RRGGBB" */
  color: string;
  style: WatermarkStyle;
}

export interface ImageWatermarkOpts {
  imagePath: string;
  /** 水印宽度占页宽比例 0.05–1 */
  scale: number;
  style: WatermarkStyle;
}

export function addTextWatermark(
  docId: number,
  pages: number[],
  opts: TextWatermarkOpts,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("add_text_watermark", { docId, pages, opts });
}

export function addImageWatermark(
  docId: number,
  pages: number[],
  opts: ImageWatermarkOpts,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("add_image_watermark", { docId, pages, opts });
}

// ---------- 深度编辑（M5） ----------

export interface PtRect {
  left: number;
  bottom: number;
  right: number;
  top: number;
}

export interface RewriteTextOpts {
  newText: string;
  fontSize: number;
  color: string;
}

export function rewriteText(
  docId: number,
  pageIndex: number,
  region: PtRect,
  opts: RewriteTextOpts,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("rewrite_text", {
    docId,
    pageIndex,
    region,
    opts,
  });
}

export interface AddTextBoxOpts {
  text: string;
  fontSize: number;
  color: string;
  x: number;
  y: number;
}

export function addTextBox(
  docId: number,
  pageIndex: number,
  opts: AddTextBoxOpts,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("add_text_box", {
    docId,
    pageIndex,
    opts,
  });
}

export function replaceImage(
  docId: number,
  pageIndex: number,
  objectIndex: number,
  newImagePath: string,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("replace_image", {
    docId,
    pageIndex,
    objectIndex,
    newImagePath,
  });
}

export function deleteImageObject(
  docId: number,
  pageIndex: number,
  objectIndex: number,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("delete_image_object", {
    docId,
    pageIndex,
    objectIndex,
  });
}

export function isScannedPage(
  docId: number,
  pageIndex: number,
): Promise<boolean> {
  return invoke<boolean>("is_scanned_page", { docId, pageIndex });
}

export function clearPageText(
  docId: number,
  pages: number[],
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("clear_page_text", { docId, pages });
}

// ---------- 水印去除（M4 part 2） ----------

export interface Rect {
  left: number;
  bottom: number;
  right: number;
  top: number;
}

export interface ObjectFingerprint {
  objectIndex: number;
  kind: string;
  left: number;
  bottom: number;
  right: number;
  top: number;
  occurrence: number;
  totalSampled: number;
}

export interface DetectResult {
  candidates: ObjectFingerprint[];
  totalPages: number;
  sampledPages: number;
}

export interface RemovedSummary {
  info: DocumentInfo;
  removedCount: number;
}

export function removeObjectsInRect(
  docId: number,
  pages: number[],
  rect: Rect,
): Promise<RemovedSummary> {
  return invoke<RemovedSummary>("remove_objects_in_rect", {
    docId,
    pages,
    rect,
  });
}

export function detectWatermarkCandidates(
  docId: number,
  samplePages?: number,
  threshold?: number,
): Promise<DetectResult> {
  return invoke<DetectResult>("detect_watermark_candidates", {
    docId,
    samplePages: samplePages ?? null,
    threshold: threshold ?? null,
  });
}

export function applyWatermarkRemoval(
  docId: number,
  selectedIndices: number[],
): Promise<RemovedSummary> {
  return invoke<RemovedSummary>("apply_watermark_removal", {
    docId,
    selectedIndices,
  });
}

// ---------- 注释（M5） ----------

export type AnnotationKind =
  | "Highlight"
  | "Underline"
  | "Strikeout"
  | "StickyNote"
  | "FreeText"
  | "Square";

export interface AnnotationInfo {
  index: number;
  pageIndex: number;
  kind: AnnotationKind;
  left: number;
  bottom: number;
  right: number;
  top: number;
  contents: string;
  color: string;
}

export interface RegionSpec {
  left: number; // 0-1
  top: number; // 0-1
  width: number; // 0-1
  height: number; // 0-1
}

export interface AddAnnotationOpts {
  kind: AnnotationKind;
  region: RegionSpec;
  contents: string;
  color: string;
  opacity: number;
}

export function listAnnotations(
  docId: number,
  pageIndex?: number | null,
): Promise<AnnotationInfo[]> {
  return invoke<AnnotationInfo[]>("list_annotations", { docId, pageIndex: pageIndex ?? null });
}

export function addAnnotation(
  docId: number,
  pageIndex: number,
  opts: AddAnnotationOpts,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("add_annotation", { docId, pageIndex, opts });
}

export function deleteAnnotation(
  docId: number,
  pageIndex: number,
  annotationIndex: number,
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("delete_annotation", { docId, pageIndex, annotationIndex });
}

export function clearAnnotations(
  docId: number,
  pages: number[],
): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("clear_annotations", { docId, pages });
}

// ---------- 文档安全（M6） ----------

export interface SecurityStatus {
  handlerRevision: "Unprotected" | "Revision2" | "Revision3" | "Revision4" | "Unknown" | string;
  canPrintHighQuality: boolean;
  canPrintLowQuality: boolean;
  canModifyDocument: boolean;
  canExtractTextAndGraphics: boolean;
  canAddAnnotations: boolean;
  canFillFormFields: boolean;
  canAssembleDocument: boolean;
  canCreateNewFormFields: boolean;
}

export function getSecurityStatus(docId: number): Promise<SecurityStatus> {
  return invoke<SecurityStatus>("get_security_status", { docId });
}

export function exportPlainCopy(docId: number, outputPath: string): Promise<string> {
  return invoke<string>("export_plain_copy", { docId, outputPath });
}

export function reloadPlain(docId: number): Promise<DocumentInfo> {
  return invoke<DocumentInfo>("reload_plain", { docId });
}

// ---------- 格式互转（M7） ----------

export interface PageRangeSpec {
  pages: number[];
  dpi: number;
  /** "png" | "jpeg" */
  format: string;
}

export interface ImageToPdfOpts {
  imagePaths: string[];
  /** "fit" | "a4" | "letter" | "auto" */
  pageSize: string;
  /** "fit" | "fill" */
  layout: string;
}

export function exportPagesToImages(
  docId: number,
  spec: PageRangeSpec,
  outputDir: string,
): Promise<string[]> {
  return invoke<string[]>("export_pages_to_images", { docId, spec, outputDir });
}

export function imagesToPdf(opts: ImageToPdfOpts, outputPath: string): Promise<string> {
  return invoke<string>("images_to_pdf", { opts, outputPath });
}

// ---------- Office 互转（M8） ----------

export interface OfficeProbe {
  installed: boolean;
  path: string | null;
  version: string | null;
}

export function detectOffice(): Promise<OfficeProbe> {
  return invoke<OfficeProbe>("detect_office");
}

export function convertOfficeToPdf(
  sofficePath: string,
  source: string,
  outputDir: string,
): Promise<string> {
  return invoke<string>("convert_office_to_pdf", {
    sofficePath,
    source,
    outputDir,
  });
}

// ---------- 电子书互转（M10） ----------

export interface EbookToolProbe {
  installed: boolean;
  path: string | null;
  version: string | null;
}

export interface ConvertEbookOpts {
  source: string;
  output: string;
  author?: string | null;
  title?: string | null;
}

export function detectEbookTools(): Promise<EbookToolProbe> {
  return invoke<EbookToolProbe>("detect_ebook_tools");
}

export function convertEbookToPdf(
  toolPath: string,
  opts: ConvertEbookOpts,
): Promise<string> {
  return invoke<string>("convert_ebook_to_pdf", { toolPath, opts });
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
