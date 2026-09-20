use pdfium_render::prelude::*;
use tauri::State;

use crate::document::{pdfium, AppState};
use crate::error::{AppError, AppResult};

/// 渲染页面为 RGBA 位图。
/// 返回二进制布局：[width:u32 LE][height:u32 LE][RGBA 像素...]，前端用 ArrayBuffer 解析。
#[tauri::command]
pub async fn render_page(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
    scale: f64,
) -> AppResult<tauri::ipc::Response> {
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&entry.bytes, None)?;
    let pages = doc.pages();
    let page = pages
        .get(page_index.try_into().map_err(|_| AppError::PageOutOfRange)?)
        .map_err(|_| AppError::PageOutOfRange)?;

    let scale = scale.clamp(0.1, 8.0);
    let width = (page.width().value as f64 * scale).round().max(1.0) as Pixels;
    let height = (page.height().value as f64 * scale).round().max(1.0) as Pixels;

    let bitmap = page.render(width, height, None)?;
    Ok(pack_bitmap(&bitmap))
}

/// 生成页面缩略图（固定目标宽度 160px）。
#[tauri::command]
pub async fn render_thumbnail(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
) -> AppResult<tauri::ipc::Response> {
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&entry.bytes, None)?;
    let pages = doc.pages();
    let page = pages
        .get(page_index.try_into().map_err(|_| AppError::PageOutOfRange)?)
        .map_err(|_| AppError::PageOutOfRange)?;

    let config = PdfRenderConfig::new().set_target_width(160);
    let bitmap = page.render_with_config(&config)?;
    Ok(pack_bitmap(&bitmap))
}

/// 位图 → [w][h][RGBA...] 二进制响应。
fn pack_bitmap(bitmap: &PdfBitmap) -> tauri::ipc::Response {
    let width = bitmap.width();
    let height = bitmap.height();
    let rgba = bitmap.as_rgba_bytes();
    let mut buf = Vec::with_capacity(8 + rgba.len());
    buf.extend_from_slice(&width.to_le_bytes());
    buf.extend_from_slice(&height.to_le_bytes());
    buf.extend_from_slice(&rgba);
    tauri::ipc::Response::new(buf)
}

/// 提取页面纯文本（供搜索与扫描版检测使用）。
#[tauri::command]
pub async fn get_page_text(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
) -> AppResult<String> {
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&entry.bytes, None)?;
    let pages = doc.pages();
    let page = pages
        .get(page_index.try_into().map_err(|_| AppError::PageOutOfRange)?)
        .map_err(|_| AppError::PageOutOfRange)?;
    let text = page.text()?;
    Ok(text.all())
}
