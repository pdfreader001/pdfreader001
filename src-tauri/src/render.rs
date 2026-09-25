use pdfium_render::prelude::*;
use serde::Serialize;
use tauri::State;

use crate::document::{pdfium, AppState};
use crate::error::{AppError, AppResult};

/// 双击命中：在指定 PDF 点 (x, y) 找到字符后，扩展到相邻同类字符，
/// 返回整段命中区域的矩形 + 命中原文 + 原字体样式。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPickResult {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
    /// 命中字符所在词/短句的原文（用于回填到重写 modal 提示）。
    pub text: String,
    /// 命中字符的字体名（pdfium 返回的 PostScript 基名，如 `ABCDEF+SimSun`）；读取失败为空串。
    pub font_name: String,
    /// 命中字符的字号（PDF 点）。
    pub font_size: f32,
    /// 命中字符的填充色 `#rrggbb`；读取失败为空串（前端应回退默认色）。
    pub color: String,
    /// 命中字符是否粗体。
    pub font_bold: bool,
    /// 命中字符是否斜体。
    pub font_italic: bool,
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

/// Pure function: compute the outer bounding box that contains all given rects.
///
/// Each rect is (left, bottom, right, top). Returns `None` if the input is empty.
pub fn bounding_box_from_rects(rects: &[(f32, f32, f32, f32)]) -> Option<(f32, f32, f32, f32)> {
    if rects.is_empty() {
        return None;
    }
    let mut min_left = f32::MAX;
    let mut min_bottom = f32::MAX;
    let mut max_right = f32::MIN;
    let mut max_top = f32::MIN;
    for &(l, b, r, t) in rects {
        min_left = min_left.min(l);
        min_bottom = min_bottom.min(b);
        max_right = max_right.max(r);
        max_top = max_top.max(t);
    }
    Some((min_left, min_bottom, max_right, max_top))
}

/// Pure function: check if a character is a "word character" for boundary expansion.
///
/// Word chars are non-whitespace and non-null. Whitespace (spaces, newlines, tabs, etc.)
/// and the null character act as word boundaries.
pub fn is_word_char(ch: char) -> bool {
    !ch.is_whitespace() && ch != '\u{0}'
}

/// Pure function: expand a hit index to word boundaries within a char slice.
///
/// Starting at `hit_idx`, expands left and right as long as adjacent characters
/// satisfy `is_word_char`. Returns inclusive `(start, end)` indices.
///
/// Returns `None` if `hit_idx` is out of bounds or the hit char itself is not a word char.
pub fn expand_word_boundary(chars: &[char], hit_idx: usize) -> Option<(usize, usize)> {
    if hit_idx >= chars.len() {
        return None;
    }
    if !is_word_char(chars[hit_idx]) {
        return None;
    }

    let mut start = hit_idx;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }

    let mut end = hit_idx;
    while end + 1 < chars.len() && is_word_char(chars[end + 1]) {
        end += 1;
    }

    Some((start, end))
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
    let _gate = crate::document::pdfium_gate();
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
    let _gate = crate::document::pdfium_gate();
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&entry.bytes, None)?;
    let pages = doc.pages();
    let page = pages
        .get(
            page_index
                .try_into()
                .map_err(|_| AppError::PageOutOfRange)?,
        )
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
    let _gate = crate::document::pdfium_gate();
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
        .get(
            page_index
                .try_into()
                .map_err(|_| AppError::PageOutOfRange)?,
        )
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
    let _gate = crate::document::pdfium_gate();
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
        .get(
            page_index
                .try_into()
                .map_err(|_| AppError::PageOutOfRange)?,
        )
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
        // Collect segment bounds then compute outer bounding box
        let mut rects: Vec<(f32, f32, f32, f32)> = Vec::new();
        for segment in segments.iter() {
            let b = segment.bounds();
            rects.push((
                b.left().value,
                b.bottom().value,
                b.right().value,
                b.top().value,
            ));
        }
        if let Some((l, b, r, t)) = bounding_box_from_rects(&rects) {
            hits.push(PageSearchHit {
                left: l,
                bottom: b,
                right: r,
                top: t,
            });
        }
    }

    Ok(PageSearchResult { page_index, hits })
}

/// 去掉 PDF 子集字体的 `ABCDEF+` 前缀（6 个大写字母 + `+`）。
pub fn strip_subset_prefix(name: &str) -> String {
    match name.split_once('+') {
        Some((prefix, rest))
            if !rest.is_empty()
                && prefix.len() == 6
                && prefix.chars().all(|c| c.is_ascii_uppercase()) =>
        {
            rest.to_string()
        }
        _ => name.to_string(),
    }
}

/// 从命中的字符读出原字体样式：`(字体名, 字号, 填充色, 粗体, 斜体)`。
///
/// 字体名与颜色在 pdfium 侧可能读取失败，此时降级为空串，由调用方回退默认样式。
fn read_char_style(c: &PdfPageTextChar) -> (String, f32, String, bool, bool) {
    let font_name = strip_subset_prefix(&c.font_name());
    let font_size = c.unscaled_font_size().value;
    let color = match c.fill_color() {
        Ok(col) => format!("#{:02x}{:02x}{:02x}", col.red(), col.green(), col.blue()),
        Err(_) => String::new(),
    };
    (
        font_name,
        font_size,
        color,
        c.font_is_bold_reenforced(),
        c.font_is_italic(),
    )
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
    let _gate = crate::document::pdfium_gate();
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
        .get(
            page_index
                .try_into()
                .map_err(|_| AppError::PageOutOfRange)?,
        )
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
        Some((
            b.left().value,
            b.bottom().value,
            b.right().value,
            b.top().value,
            ch,
        ))
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
    let (font_name, font_size, color, font_bold, font_italic) = read_char_style(&hit_char);

    // "词字符" = 非空白、非控制字符；空白处只返回单字符

    if !is_word_char(hit_ch) {
        return Ok(Some(TextPickResult {
            left: l,
            bottom: b,
            right: r,
            top: t,
            text: hit_ch.to_string(),
            font_name,
            font_size,
            color,
            font_bold,
            font_italic,
        }));
    }

    // 向左扩展到词起点（遇到空白或不可读字符就停）
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
    let mut rects: Vec<(f32, f32, f32, f32)> = Vec::new();
    let mut word = String::with_capacity(r_end - l_end + 1);
    for i in l_end..=r_end {
        let c = page_chars.get(i).unwrap();
        let Some((cl, cb, cr, ct, ch)) = read_char(&c) else {
            continue;
        };
        rects.push((cl, cb, cr, ct));
        word.push(ch);
    }
    let (mn_l, mn_b, mx_r, mx_t) = bounding_box_from_rects(&rects).unwrap_or((l, b, r, t));

    Ok(Some(TextPickResult {
        left: mn_l,
        bottom: mn_b,
        right: mx_r,
        top: mx_t,
        text: word,
        font_name,
        font_size,
        color,
        font_bold,
        font_italic,
    }))
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    // ------------------------------------------------------------------
    // bounding_box_from_rects
    // ------------------------------------------------------------------

    #[test]
    fn test_bounding_box_empty() {
        assert!(bounding_box_from_rects(&[]).is_none());
    }

    #[test]
    fn test_bounding_box_single() {
        let rects = [(10.0, 20.0, 30.0, 40.0)];
        let (l, b, r, t) = bounding_box_from_rects(&rects).unwrap();
        assert!(approx(l, 10.0));
        assert!(approx(b, 20.0));
        assert!(approx(r, 30.0));
        assert!(approx(t, 40.0));
    }

    #[test]
    fn test_bounding_box_two_overlapping() {
        let rects = [(0.0, 0.0, 10.0, 10.0), (5.0, 5.0, 15.0, 15.0)];
        let (l, b, r, t) = bounding_box_from_rects(&rects).unwrap();
        assert!(approx(l, 0.0));
        assert!(approx(b, 0.0));
        assert!(approx(r, 15.0));
        assert!(approx(t, 15.0));
    }

    #[test]
    fn test_bounding_box_separate() {
        let rects = [(0.0, 0.0, 5.0, 5.0), (20.0, 30.0, 25.0, 35.0)];
        let (l, b, r, t) = bounding_box_from_rects(&rects).unwrap();
        assert!(approx(l, 0.0));
        assert!(approx(b, 0.0));
        assert!(approx(r, 25.0));
        assert!(approx(t, 35.0));
    }

    #[test]
    fn test_bounding_box_nested() {
        let rects = [
            (0.0, 0.0, 100.0, 100.0),
            (20.0, 20.0, 80.0, 80.0),
            (40.0, 40.0, 60.0, 60.0),
        ];
        let (l, b, r, t) = bounding_box_from_rects(&rects).unwrap();
        assert!(approx(l, 0.0));
        assert!(approx(b, 0.0));
        assert!(approx(r, 100.0));
        assert!(approx(t, 100.0));
    }

    #[test]
    fn test_bounding_box_negative_coords() {
        let rects = [(-10.0, -5.0, 5.0, 10.0), (-3.0, -20.0, 15.0, -8.0)];
        let (l, b, r, t) = bounding_box_from_rects(&rects).unwrap();
        assert!(approx(l, -10.0));
        assert!(approx(b, -20.0));
        assert!(approx(r, 15.0));
        assert!(approx(t, 10.0));
    }

    // ------------------------------------------------------------------
    // is_word_char
    // ------------------------------------------------------------------

    #[test]
    fn test_is_word_char_letters() {
        assert!(is_word_char('a'));
        assert!(is_word_char('Z'));
        assert!(is_word_char('é'));
        assert!(is_word_char('中'));
    }

    #[test]
    fn test_is_word_char_digits() {
        assert!(is_word_char('0'));
        assert!(is_word_char('9'));
    }

    #[test]
    fn test_is_word_char_punctuation() {
        // Punctuation is also non-whitespace, so counted as word char
        assert!(is_word_char('.'));
        assert!(is_word_char('-'));
        assert!(is_word_char('_'));
        assert!(is_word_char(','));
    }

    #[test]
    fn test_is_word_char_whitespace() {
        assert!(!is_word_char(' '));
        assert!(!is_word_char('\t'));
        assert!(!is_word_char('\n'));
        assert!(!is_word_char('\r'));
    }

    #[test]
    fn test_is_word_char_null() {
        assert!(!is_word_char('\u{0}'));
    }

    // ------------------------------------------------------------------
    // expand_word_boundary
    // ------------------------------------------------------------------

    #[test]
    fn test_expand_word_empty_chars() {
        assert!(expand_word_boundary(&[], 0).is_none());
    }

    #[test]
    fn test_expand_word_out_of_bounds() {
        let chars: Vec<char> = vec!['a', 'b', 'c'];
        assert!(expand_word_boundary(&chars, 5).is_none());
    }

    #[test]
    fn test_expand_word_hit_whitespace() {
        let chars: Vec<char> = "hello world".chars().collect();
        // hit space at index 5
        assert!(expand_word_boundary(&chars, 5).is_none());
    }

    #[test]
    fn test_expand_word_middle_of_word() {
        let chars: Vec<char> = "hello".chars().collect();
        let (start, end) = expand_word_boundary(&chars, 2).unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 4);
    }

    #[test]
    fn test_expand_word_start_of_word() {
        let chars: Vec<char> = "hello world".chars().collect();
        let (start, end) = expand_word_boundary(&chars, 0).unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 4); // "hello" ends at index 4
    }

    #[test]
    fn test_expand_word_end_of_word() {
        let chars: Vec<char> = "hello world".chars().collect();
        let (start, end) = expand_word_boundary(&chars, 4).unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 4);
    }

    #[test]
    fn test_expand_word_second_word() {
        let chars: Vec<char> = "hello world".chars().collect();
        // 'w' at index 6
        let (start, end) = expand_word_boundary(&chars, 6).unwrap();
        assert_eq!(start, 6);
        assert_eq!(end, 10); // "world"
    }

    #[test]
    fn test_expand_word_single_char() {
        let chars: Vec<char> = "a b c".chars().collect();
        let (start, end) = expand_word_boundary(&chars, 2).unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 2);
    }

    #[test]
    fn test_expand_word_cjk() {
        // CJK characters are non-whitespace, so they form one continuous "word"
        let chars: Vec<char> = "你好世界".chars().collect();
        let (start, end) = expand_word_boundary(&chars, 1).unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 3);
    }

    #[test]
    fn test_expand_word_with_punctuation() {
        // Punctuation is a word char, so it's included in expansion
        let chars: Vec<char> = "hello-world".chars().collect();
        let (start, end) = expand_word_boundary(&chars, 5).unwrap(); // '-'
        assert_eq!(start, 0);
        assert_eq!(end, 10);
    }
}
