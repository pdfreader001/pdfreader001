use pdfium_render::prelude::*;
use serde::Serialize;
use tauri::State;

use crate::document::{pdfium, AppState};
use crate::error::{AppError, AppResult};

/// 双击命中：在指定 PDF 点 (x, y) 找到字符后，扩展到相邻同类字符，
/// 返回整段命中区域的矩形 + 命中原文。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPickResult {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
    /// 命中字符所在词/短句的原文（用于回填到重写 modal 提示）。
    pub text: String,
}

/// 单页搜索结果：一页内所有命中及其矩形坐标。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageSearchResult {
    pub page_index: u32,
    pub hits: Vec<PageSearchHit>,
}

/// 单个命中的矩形（PDF 点，左下原点）。
#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct PageSearchHit {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

/// 渲染页面为 RGBA 位图（纯函数版本）：输入 bytes，返回 [w:u32][h:u32][RGBA...]。
/// 返回 tauri::ipc::Response 方便直接作为 command 输出。
pub fn render_page_logic(
    pdfium: &Pdfium,
    bytes: &[u8],
    page_index: u32,
    scale: f64,
) -> AppResult<tauri::ipc::Response> {
    let doc = pdfium.load_pdf_from_byte_slice(bytes, None)?;
    let page_count = doc.pages().len() as u32;
    if page_index >= page_count {
        return Err(AppError::PageOutOfRange);
    }
    let page = doc.pages().get(page_index as u16)?;
    let scale = scale.clamp(0.1, 8.0);
    let width = (page.width().value as f64 * scale).round().max(1.0) as Pixels;
    let height = (page.height().value as f64 * scale).round().max(1.0) as Pixels;
    let bitmap = page.render(width, height, None)?;
    Ok(pack_bitmap(&bitmap))
}

/// 渲染页面为 RGBA 位图。
/// 返回二进制布局：[width:u32 LE][height:u32 LE][RGBA 像素...]，前端用 ArrayBuffer 解析。
#[tauri::command]
pub async fn render_page(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
    scale: f64,
) -> AppResult<tauri::ipc::Response> {
    let pdfium = pdfium();
    let bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        entry.bytes.clone()
    };
    // 注意：bytes 已拷贝后释放锁，渲染期间不会被并发修改阻塞其他命令。
    // pdfium 渲染使用本地拷贝，与原 bytes 解耦，无需担心一致性。
    render_page_logic(pdfium, &bytes, page_index, scale)
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
    get_page_text_logic(&entry.bytes, page_index)
}

/// 纯函数版本：直接对 PDF 字节做单页文本提取。
pub fn get_page_text_logic(bytes: &[u8], page_index: u32) -> AppResult<String> {
    let pdfium = pdfium();
    let doc = pdfium
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|_| AppError::Damaged)?;
    let pages = doc.pages();
    let page = pages
        .get(page_index.try_into().map_err(|_| AppError::PageOutOfRange)?)
        .map_err(|_| AppError::PageOutOfRange)?;
    let text = page.text()?;
    Ok(text.all())
}

/// 在指定页面内搜索关键词，返回每个命中的矩形坐标（PDF 点，左下原点）。
/// 不区分大小写；单页最多返回 max_hits 个命中（默认 50）。
/// 用于画布上的搜索高亮显示。
#[tauri::command]
pub async fn search_page_text(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
    query: String,
    max_hits: Option<u32>,
) -> AppResult<PageSearchResult> {
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    search_page_text_logic(&entry.bytes, page_index, &query, max_hits)
}

/// 纯函数版本：直接对 PDF 字节做单页搜索，返回命中矩形。
/// 供命令层与测试共用。
pub fn search_page_text_logic(
    bytes: &[u8],
    page_index: u32,
    query: &str,
    max_hits: Option<u32>,
) -> AppResult<PageSearchResult> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(PageSearchResult {
            page_index,
            hits: vec![],
        });
    }
    let max_hits = max_hits.unwrap_or(50) as usize;

    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(bytes, None)?;
    let pages = doc.pages();
    let page = pages
        .get(page_index.try_into().map_err(|_| AppError::PageOutOfRange)?)
        .map_err(|_| AppError::PageOutOfRange)?;

    let page_text = page.text()?;

    // 使用 pdfium 原生搜索，通过结果段的 bounds 合并得到命中矩形
    let options = PdfSearchOptions::new(); // 不区分大小写、不整词匹配
    let search = match page_text.search(&query, &options) {
        Ok(s) => s,
        Err(_) => {
            return Ok(PageSearchResult {
                page_index,
                hits: vec![],
            });
        }
    };

    let mut hits: Vec<PageSearchHit> = Vec::new();

    for segments in search.iter(PdfSearchDirection::SearchForward) {
        if hits.len() >= max_hits {
            break;
        }
        // 合并所有命中段的 bounds（跨多行/多文本段时为外包围盒）
        let mut min_left = f32::MAX;
        let mut min_bottom = f32::MAX;
        let mut max_right = f32::MIN;
        let mut max_top = f32::MIN;
        let mut has_bounds = false;

        for segment in segments.iter() {
            let b = segment.bounds();
            has_bounds = true;
            let l = b.left().value;
            let r = b.right().value;
            let bt = b.bottom().value;
            let t = b.top().value;
            min_left = min_left.min(l);
            min_bottom = min_bottom.min(bt);
            max_right = max_right.max(r);
            max_top = max_top.max(t);
        }

        if !has_bounds {
            continue;
        }

        hits.push(PageSearchHit {
            left: min_left,
            bottom: min_bottom,
            right: max_right,
            top: max_top,
        });
    }

    Ok(PageSearchResult { page_index, hits })
}

/// 给定 PDF 点 (x, y)，找到该位置的字符并扩展为相邻同类字符的连续片段，
/// 返回片段外包围盒 + 原文。用于画布双击文字后自动选中词组。
#[tauri::command]
pub async fn pick_text_at_point(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
    x: f32,
    y: f32,
) -> AppResult<Option<TextPickResult>> {
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    pick_text_at_point_logic(&entry.bytes, page_index, x, y)
}

/// 纯函数版本。
pub fn pick_text_at_point_logic(
    bytes: &[u8],
    page_index: u32,
    x: f32,
    y: f32,
) -> AppResult<Option<TextPickResult>> {
    let pdfium = pdfium();
    let doc = pdfium
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|_| AppError::Damaged)?;
    let pages = doc.pages();
    let page = pages
        .get(page_index.try_into().map_err(|_| AppError::PageOutOfRange)?)
        .map_err(|_| AppError::PageOutOfRange)?;
    let text = match page.text() {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };
    let page_chars = text.chars();
    let count = page_chars.len();

    // 工具：从 PdfPageTextChar 取 (l, b, r, t, ch)，失败/缺字符返回 None
    fn read_char(
        c: &pdfium_render::prelude::PdfPageTextChar,
    ) -> Option<(f32, f32, f32, f32, char)> {
        let b = c.tight_bounds().ok()?;
        let ch = c.unicode_char()?;
        Some((b.left().value, b.bottom().value, b.right().value, b.top().value, ch))
    }

    // 1. 找到 hit 字符的索引
    let mut hit_idx: Option<usize> = None;
    for (i, c) in page_chars.iter().enumerate() {
        if let Some((l, b, r, t, _)) = read_char(&c) {
            if x >= l && x <= r && y >= b && y <= t {
                hit_idx = Some(i);
                break;
            }
        }
    }
    let Some(hit) = hit_idx else {
        return Ok(None);
    };

    let hit_char = page_chars.get(hit)?;
    let Some((l, b, r, t, hit_ch)) = read_char(&hit_char) else {
        return Ok(None);
    };

    // "词字符" = 非空白、非控制字符；空白处只返回单字符
    let is_word_char = |ch: char| !ch.is_whitespace() && ch != '\u{0}';

    if !is_word_char(hit_ch) {
        return Ok(Some(TextPickResult {
            left: l,
            bottom: b,
            right: r,
            top: t,
            text: hit_ch.to_string(),
        }));
    }

    // 向左扩展到词起点（遇到空白就停）
    let mut l_end = hit;
    while l_end > 0 {
        let prev = page_chars.get(l_end - 1)?;
        let Some((_, _, _, _, prev_ch)) = read_char(&prev) else {
            break;
        };
        if !is_word_char(prev_ch) {
            break;
        }
        l_end -= 1;
    }
    // 向右扩展到词终点
    let mut r_end = hit;
    while r_end + 1 < count {
        let next = page_chars.get(r_end + 1)?;
        let Some((_, _, _, _, next_ch)) = read_char(&next) else {
            break;
        };
        if !is_word_char(next_ch) {
            break;
        }
        r_end += 1;
    }

    // 计算外包围盒 + 拼接原文
    let mut mn_l = f32::MAX;
    let mut mx_r = f32::MIN;
    let mut mn_b = f32::MAX;
    let mut mx_t = f32::MIN;
    let mut word = String::with_capacity(r_end - l_end + 1);
    for i in l_end..=r_end {
        let c = page_chars.get(i).unwrap();
        let Some((cl, cb, cr, ct, ch)) = read_char(&c) else {
            continue;
        };
        mn_l = mn_l.min(cl);
        mx_r = mx_r.max(cr);
        mn_b = mn_b.min(cb);
        mx_t = mx_t.max(ct);
        word.push(ch);
    }

    Ok(Some(TextPickResult {
        left: mn_l,
        bottom: mn_b,
        right: mx_r,
        top: mx_t,
        text: word,
    }))
}
