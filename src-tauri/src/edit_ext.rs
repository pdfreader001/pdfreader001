//! M5 深度编辑：文字遮盖-重写、新增文本/图片、扫描版检测
//!
//! 简化策略：
//! - 文字重写：对页面所有文本对象做 AABB 求并 → 覆盖白色矩形 → 删除文本对象 → 按字号插入新文本
//! - 新增文本/图片：用户给出坐标与内容 → 创建新对象
//! - 扫描版检测：用 `page.text().all().trim().is_empty()` 粗判
//!
//! 字体缺失兜底：调用现有 `watermark::load_font_for_text` 自动回退到系统中文字体

use std::path::Path;

use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::document::{pdfium as get_pdfium, push_snapshot, AppState, DocumentInfo};
use crate::error::{AppError, AppResult};
use crate::pages::{commit_and_return, load_doc, normalize_indices};
use crate::watermark::load_font_for_text;

/// PDF 点坐标矩形（左下原点）。M4 复用了一个 Rect 类型，本模块独立以避免依赖耦合。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtRect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl PtRect {
    pub(crate) fn valid(&self) -> bool {
        self.right > self.left && self.top > self.bottom
    }
    pub(crate) fn width(&self) -> f32 {
        (self.right - self.left).max(0.0)
    }
    pub(crate) fn height(&self) -> f32 {
        (self.top - self.bottom).max(0.0)
    }
    /// AABB intersection test (strict): returns true iff the two rects have non-zero overlap.
    pub(crate) fn intersects(&self, other: &PtRect) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.bottom < other.top
            && self.top > other.bottom
    }
    /// Returns true iff `point` (x, y) lies within this rect (inclusive on edges).
    pub(crate) fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.left && x <= self.right && y >= self.bottom && y <= self.top
    }
}

fn parse_hex_color(hex: &str) -> PdfColor {
    let h = hex.trim_start_matches('#');
    let byte = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0);
    let (r, g, b) = match h.len() {
        6 => (byte(&h[0..2]), byte(&h[2..4]), byte(&h[4..6])),
        _ => (0, 0, 0),
    };
    PdfColor::new(r, g, b, 255)
}

/// 文字重写选项。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteTextOpts {
    /// 要替换的新文本内容。
    pub new_text: String,
    /// 字号（pt）。
    pub font_size: f32,
    /// 颜色（"#RRGGBB"）。
    pub color: String,
}

/// 新增文本框选项。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddTextBoxOpts {
    pub text: String,
    /// 字号（pt）。
    pub font_size: f32,
    /// 颜色（"#RRGGBB"）。
    pub color: String,
    /// 左下角 x 坐标（pt）。
    pub x: f32,
    /// 左下角 y 坐标（pt）。
    pub y: f32,
}

/// 文字重写核心逻辑（纯函数版本）：输入 bytes，返回新 bytes。
pub fn rewrite_text_logic(
    pdfium: &Pdfium,
    bytes: &[u8],
    page_index: u32,
    region: PtRect,
    opts: &RewriteTextOpts,
) -> AppResult<Vec<u8>> {
    if !region.valid() {
        return Err(AppError::InvalidRect);
    }
    if opts.new_text.is_empty() {
        return Err(AppError::TextEmpty);
    }
    let mut doc = pdfium.load_pdf_from_byte_slice(bytes, None)?;
    let page_count = doc.pages().len() as u32;
    if page_index >= page_count {
        return Err(AppError::PageOutOfRange);
    }
    let font_token = load_font_for_text(&mut doc, &opts.new_text)?;
    let color = parse_hex_color(&opts.color);
    let font_size = opts.font_size.max(4.0);

    {
        let pages_col = doc.pages_mut();
        let mut page = pages_col.get(page_index as u16)?;

        // 1. 收集 region 内的所有文本对象索引（从后往前删）
        let mut to_remove: Vec<u32> = Vec::new();
        {
            let objs = page.objects();
            for (i, obj) in objs.iter().enumerate() {
                if !matches!(obj.object_type(), PdfPageObjectType::Text) {
                    continue;
                }
                let Ok(bounds) = obj.bounds() else { continue };
                let (l, b, r, t) = (
                    bounds.left().value as f32,
                    bounds.bottom().value as f32,
                    bounds.right().value as f32,
                    bounds.top().value as f32,
                );
                let obj_rect = PtRect {
                    left: l.min(r),
                    bottom: b.min(t),
                    right: l.max(r),
                    top: b.max(t),
                };
                if region.intersects(&obj_rect) {
                    to_remove.push(i as u32);
                }
            }
        }
        for &i in to_remove.iter().rev() {
            let _ = page.objects_mut().remove_object_at_index(i as usize);
        }

        // 2. 在 region 上叠加白色矩形（视觉遮盖）
        let white = PdfColor::new(255, 255, 255, 255);
        let pdf_rect = PdfRect::new(
            PdfPoints::new(region.bottom),
            PdfPoints::new(region.left),
            PdfPoints::new(region.top),
            PdfPoints::new(region.right),
        );
        {
            let mut objects = page.objects_mut();
            let _rect = objects.create_path_object_rect(
                pdf_rect,
                None, // 无描边
                None, // 无描边宽度
                Some(white),
            )?;
        }

        // 3. 在 region 底部插入新文字
        let pad_x = region.width() * 0.05;
        let text_x = region.left + pad_x;
        let text_y = region.bottom + region.height() * 0.15;
        {
            let mut objects = page.objects_mut();
            let mut obj = objects.create_text_object(
                PdfPoints::new(text_x),
                PdfPoints::new(text_y),
                &opts.new_text,
                font_token,
                PdfPoints::new(font_size),
            )?;
            obj.set_fill_color(color)?;
        }
    }
    Ok(doc.save_to_bytes()?)
}

/// 新增文本框纯函数版本。
pub fn add_text_box_logic(
    pdfium: &Pdfium,
    bytes: &[u8],
    page_index: u32,
    opts: &AddTextBoxOpts,
) -> AppResult<Vec<u8>> {
    if opts.text.is_empty() {
        return Err(AppError::TextEmpty);
    }
    let mut doc = pdfium.load_pdf_from_byte_slice(bytes, None)?;
    let page_count = doc.pages().len() as u32;
    if page_index >= page_count {
        return Err(AppError::PageOutOfRange);
    }
    let font_token = load_font_for_text(&mut doc, &opts.text)?;
    let color = parse_hex_color(&opts.color);
    let font_size = opts.font_size.max(4.0);
    {
        let pages_col = doc.pages_mut();
        let mut page = pages_col.get(page_index as u16)?;
        let mut objects = page.objects_mut();
        let mut obj = objects.create_text_object(
            PdfPoints::new(opts.x),
            PdfPoints::new(opts.y),
            &opts.text,
            font_token,
            PdfPoints::new(font_size),
        )?;
        obj.set_fill_color(color)?;
    }
    Ok(doc.save_to_bytes()?)
}

/// 文字重写 Tauri command。
#[tauri::command]
pub async fn rewrite_text(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
    region: PtRect,
    opts: RewriteTextOpts,
) -> AppResult<DocumentInfo> {
    if !region.valid() {
        return Err(AppError::InvalidRect);
    }
    if opts.new_text.is_empty() {
        return Err(AppError::TextEmpty);
    }
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        rewrite_text_logic(pdfium, &entry.bytes, page_index, region, &opts)?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

#[tauri::command]
pub async fn add_text_box(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
    opts: AddTextBoxOpts,
) -> AppResult<DocumentInfo> {
    if opts.text.is_empty() {
        return Err(AppError::TextEmpty);
    }
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        add_text_box_logic(pdfium, &entry.bytes, page_index, &opts)?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

/// 图片替换：在指定页面的指定对象索引处用新图片替换。
#[tauri::command]
pub async fn replace_image(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
    object_index: u32,
    new_image_path: String,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        let img = image::open(&new_image_path)
            .map_err(|_| AppError::ImageReadFailed { path: new_image_path.clone() })?;
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        if iw < 1.0 || ih < 1.0 {
            return Err(AppError::InvalidImageSize);
        }

        // 先取出原图的 bounds（删除前），用于定位新图
        let bounds: Option<(f32, f32, f32, f32)> = {
            let pages = doc.pages();
            let Ok(page) = pages.get(page_index as u16) else {
                return Err(AppError::PageOutOfRange);
            };
            let objs = page.objects();
            objs.get(object_index as usize).ok().and_then(|obj| {
                obj.bounds().ok().map(|b| {
                    (
                        b.left().value as f32,
                        b.bottom().value as f32,
                        b.right().value as f32,
                        b.top().value as f32,
                    )
                })
            })
        };
        let (l, b, r, t) = match bounds {
            Some(v) => v,
            None => (0.0, 0.0, iw, ih),
        };
        let new_w = (r - l).abs().max(1.0);
        let new_h = (t - b).abs().max(1.0);

        {
            let pages_col = doc.pages_mut();
            let mut page = pages_col.get(page_index as u16)?;
            // 删除原图
            let _ = page
                .objects_mut()
                .remove_object_at_index(object_index as usize);
            // 在原位置插入新图
            let mut objects = page.objects_mut();
            objects.create_image_object(
                PdfPoints::new(l.min(r)),
                PdfPoints::new(b.min(t)),
                &img,
                Some(PdfPoints::new(new_w)),
                Some(PdfPoints::new(new_h)),
            )?;
        }
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

/// 删除图片对象。
#[tauri::command]
pub async fn delete_image_object(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
    object_index: u32,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        {
            let pages_col = doc.pages_mut();
            let mut page = pages_col.get(page_index as u16)?;
            page.objects_mut()
                .remove_object_at_index(object_index as usize)
                .map_err(|_| AppError::PageOutOfRange)?;
        }
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

/// 页面上的图片对象信息（包围盒为 PDF 点坐标，左下原点）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageObjectInfo {
    pub object_index: u32,
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

/// 归一化包围盒：保证 left <= right、bottom <= top。
pub(crate) fn normalize_bounds(l: f32, b: f32, r: f32, t: f32) -> (f32, f32, f32, f32) {
    (l.min(r), b.min(t), l.max(r), b.max(t))
}

/// 计算把当前包围盒映射到目标矩形的增量变换矩阵。
///
/// 采用 PDF 行向量约定（`p * M`）：先平移 `(-l, -b)` 使包围盒左下角落到原点，
/// 再按 `sx/sy` 缩放，最后平移到目标矩形左下角。因此该矩阵把
/// `(l, b)` 映射到 `(target.left, target.bottom)`、`(r, t)` 映射到
/// `(target.right, target.top)`，与对象原有的旋转/倾斜无关
/// （组合方式为 `m_old * delta`，即对页面空间中的每一点施加同一仿射变换）。
///
/// 包围盒退化（宽或高为 0）时返回 `None`。
pub(crate) fn bounds_delta_matrix(
    l: f32,
    b: f32,
    r: f32,
    t: f32,
    target: &PtRect,
) -> Option<PdfMatrix> {
    let w = r - l;
    let h = t - b;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let sx = target.width() / w;
    let sy = target.height() / h;
    let to_origin = PdfMatrix::new(1.0, 0.0, 0.0, 1.0, -l, -b);
    let scale = PdfMatrix::new(sx, 0.0, 0.0, sy, 0.0, 0.0);
    let to_target = PdfMatrix::new(1.0, 0.0, 0.0, 1.0, target.left, target.bottom);
    Some(to_origin.multiply(scale).multiply(to_target))
}

/// 读取本页所有图片对象的索引与包围盒，供前端「选中」使用。
#[tauri::command]
pub async fn list_image_objects(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
) -> AppResult<Vec<ImageObjectInfo>> {
    let pdfium = get_pdfium();
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    list_image_objects_logic(pdfium, &entry.bytes, page_index)
}

/// Pure helper used by both the command and the integration tests.
pub fn list_image_objects_logic(
    pdfium: &Pdfium,
    bytes: &[u8],
    page_index: u32,
) -> AppResult<Vec<ImageObjectInfo>> {
    let doc = load_doc(pdfium, bytes)?;
    let pages = doc.pages();
    let page = pages
        .get(u16::try_from(page_index).map_err(|_| AppError::PageOutOfRange)?)
        .map_err(|_| AppError::PageOutOfRange)?;
    let mut out = Vec::new();
    for (i, obj) in page.objects().iter().enumerate() {
        if !matches!(obj.object_type(), PdfPageObjectType::Image) {
            continue;
        }
        let Ok(b) = obj.bounds() else { continue };
        let (l, bb, r, t) = normalize_bounds(
            b.left().value as f32,
            b.bottom().value as f32,
            b.right().value as f32,
            b.top().value as f32,
        );
        out.push(ImageObjectInfo {
            object_index: i as u32,
            left: l,
            bottom: bb,
            right: r,
            top: t,
        });
    }
    Ok(out)
}

/// 移动/缩放图片对象：把对象当前包围盒精确映射到目标矩形。
#[tauri::command]
pub async fn set_image_bounds(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
    object_index: u32,
    target: PtRect,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        set_image_bounds_logic(pdfium, &entry.bytes, page_index, object_index, target)?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

/// Pure helper used by both the command and the integration tests.
pub fn set_image_bounds_logic(
    pdfium: &Pdfium,
    bytes: &[u8],
    page_index: u32,
    object_index: u32,
    target: PtRect,
) -> AppResult<Vec<u8>> {
    if !target.valid() {
        return Err(AppError::InvalidRect);
    }
    let mut doc = load_doc(pdfium, bytes)?;
    {
        let pages_col = doc.pages_mut();
        let mut page = pages_col
            .get(u16::try_from(page_index).map_err(|_| AppError::PageOutOfRange)?)
            .map_err(|_| AppError::PageOutOfRange)?;
        let objects = page.objects_mut();
        let mut obj = objects
            .get(object_index as usize)
            .map_err(|_| AppError::PageOutOfRange)?;
        if !matches!(obj.object_type(), PdfPageObjectType::Image) {
            return Err(AppError::ObjectNotImage);
        }
        let bounds = obj.bounds()?;
        let (l, b, r, t) = normalize_bounds(
            bounds.left().value as f32,
            bounds.bottom().value as f32,
            bounds.right().value as f32,
            bounds.top().value as f32,
        );
        let old_matrix = obj.matrix()?;
        let delta = bounds_delta_matrix(l, b, r, t, &target).ok_or(AppError::InvalidRect)?;
        obj.reset_matrix(old_matrix.multiply(delta))?;
    }
    Ok(doc.save_to_bytes()?)
}

/// 扫描版检测：判断页面是否几乎没有可提取文本。
/// 返回 true 表示疑似扫描版（无文本层）。
#[tauri::command]
pub async fn is_scanned_page(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
) -> AppResult<bool> {
    let pdfium = get_pdfium();
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    let doc = load_doc(pdfium, &entry.bytes)?;
    Ok(is_scanned_page_logic(&doc, page_index))
}

/// Pure helper used by both the command and the integration tests.
/// `is_scanned_page_logic(doc, page_index)` returns `Ok(true)` when the page
/// has fewer than 8 characters of extractable text.
pub fn is_scanned_page_logic(
    doc: &pdfium_render::prelude::PdfDocument,
    page_index: u32,
) -> bool {
    let pages = doc.pages();
    let Ok(page) = pages.get(page_index as u16) else {
        return true;
    };
    let text = page.text().map(|t| t.all()).unwrap_or_default();
    text.trim().chars().count() < 8
}

/// 删除页面所有文本对象（用于"清空文字"等场景，但保留图片和路径）。
#[tauri::command]
pub async fn clear_page_text(
    state: State<'_, AppState>,
    doc_id: u64,
    pages: Vec<u32>,
) -> AppResult<DocumentInfo> {
    if pages.is_empty() {
        return Err(AppError::NoPagesToExport);
    }
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        let total = doc.pages().len() as u32;
        let indices = normalize_indices(&pages, total)?;
        {
            let pages_col = doc.pages_mut();
            for idx in indices {
                let mut page = pages_col.get(idx)?;
                let mut to_remove: Vec<u32> = Vec::new();
                {
                    let objs = page.objects();
                    for (i, obj) in objs.iter().enumerate() {
                        if matches!(obj.object_type(), PdfPageObjectType::Text) {
                            to_remove.push(i as u32);
                        }
                    }
                }
                for &i in to_remove.iter().rev() {
                    let _ = page.objects_mut().remove_object_at_index(i as usize);
                }
            }
        }
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

/// 辅助：判断路径是否存在（仅用于前端 UI 流程的兼容性保留）。
#[allow(dead_code)]
fn _path_exists_marker(p: &str) -> bool {
    Path::new(p).exists()
}
#[cfg(test)]
mod tests {
    //! Unit tests for hex color parsing used by rewrite_text / add_text_box.

    use super::*;

    /// parse_hex_color: accepts 6-char hex with hash.
    #[test]
    fn parse_hex_color_basic() {
        let c = parse_hex_color("#ff8040");
        assert_eq!(c.red(), 0xff);
        assert_eq!(c.green(), 0x80);
        assert_eq!(c.blue(), 0x40);
        assert_eq!(c.alpha(), 255);
    }

    /// parse_hex_color: accepts 6-char hex without hash.
    #[test]
    fn parse_hex_color_without_hash() {
        let c = parse_hex_color("ff8040");
        assert_eq!(c.red(), 0xff);
        assert_eq!(c.green(), 0x80);
        assert_eq!(c.blue(), 0x40);
    }

    /// parse_hex_color: short / malformed inputs fall back to black (0,0,0).
    #[test]
    fn parse_hex_color_invalid_is_black() {
        let c = parse_hex_color("#abc"); // 3-char short hex
        assert_eq!(c.red(), 0);
        assert_eq!(c.green(), 0);
        assert_eq!(c.blue(), 0);

        let c = parse_hex_color("zzzzzz"); // 6 chars but invalid hex digits -> byte=0
        assert_eq!(c.red(), 0);
        assert_eq!(c.green(), 0);
        assert_eq!(c.blue(), 0);

        let c = parse_hex_color(""); // empty
        assert_eq!(c.red(), 0);
    }

    // ------------------------------------------------------------------
    // PtRect: valid / width / height
    // ------------------------------------------------------------------

    #[test]
    fn pt_rect_valid_true() {
        let r = PtRect { left: 0.0, bottom: 0.0, right: 10.0, top: 10.0 };
        assert!(r.valid());
    }

    #[test]
    fn pt_rect_valid_zero_width() {
        let r = PtRect { left: 5.0, bottom: 0.0, right: 5.0, top: 10.0 };
        assert!(!r.valid());
    }

    #[test]
    fn pt_rect_valid_zero_height() {
        let r = PtRect { left: 0.0, bottom: 5.0, right: 10.0, top: 5.0 };
        assert!(!r.valid());
    }

    #[test]
    fn pt_rect_valid_inverted() {
        let r = PtRect { left: 10.0, bottom: 10.0, right: 0.0, top: 0.0 };
        assert!(!r.valid());
    }

    #[test]
    fn pt_rect_width_positive() {
        let r = PtRect { left: 10.0, bottom: 0.0, right: 50.0, top: 0.0 };
        assert_eq!(r.width(), 40.0);
    }

    #[test]
    fn pt_rect_width_negative_clamps_zero() {
        let r = PtRect { left: 50.0, bottom: 0.0, right: 10.0, top: 0.0 };
        assert_eq!(r.width(), 0.0);
    }

    #[test]
    fn pt_rect_height_positive() {
        let r = PtRect { left: 0.0, bottom: 10.0, right: 0.0, top: 50.0 };
        assert_eq!(r.height(), 40.0);
    }

    #[test]
    fn pt_rect_height_negative_clamps_zero() {
        let r = PtRect { left: 0.0, bottom: 50.0, right: 0.0, top: 10.0 };
        assert_eq!(r.height(), 0.0);
    }

    // ------------------------------------------------------------------
    // PtRect::intersects
    // ------------------------------------------------------------------

    #[test]
    fn pt_rect_intersects_overlap() {
        let a = PtRect { left: 0.0, bottom: 0.0, right: 10.0, top: 10.0 };
        let b = PtRect { left: 5.0, bottom: 5.0, right: 15.0, top: 15.0 };
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn pt_rect_intersects_contained() {
        let outer = PtRect { left: 0.0, bottom: 0.0, right: 20.0, top: 20.0 };
        let inner = PtRect { left: 5.0, bottom: 5.0, right: 15.0, top: 15.0 };
        assert!(outer.intersects(&inner));
        assert!(inner.intersects(&outer));
    }

    #[test]
    fn pt_rect_intersects_separate_x() {
        let a = PtRect { left: 0.0, bottom: 0.0, right: 5.0, top: 10.0 };
        let b = PtRect { left: 10.0, bottom: 0.0, right: 15.0, top: 10.0 };
        assert!(!a.intersects(&b));
    }

    #[test]
    fn pt_rect_intersects_separate_y() {
        let a = PtRect { left: 0.0, bottom: 0.0, right: 10.0, top: 5.0 };
        let b = PtRect { left: 0.0, bottom: 10.0, right: 10.0, top: 15.0 };
        assert!(!a.intersects(&b));
    }

    #[test]
    fn pt_rect_intersects_touch_not_overlap() {
        // Touching at edge: strict inequality -> no overlap
        let a = PtRect { left: 0.0, bottom: 0.0, right: 10.0, top: 10.0 };
        let b = PtRect { left: 10.0, bottom: 0.0, right: 20.0, top: 10.0 };
        assert!(!a.intersects(&b));
    }

    #[test]
    fn pt_rect_intersects_identical() {
        let a = PtRect { left: 1.0, bottom: 2.0, right: 3.0, top: 4.0 };
        assert!(a.intersects(&a));
    }

    // ------------------------------------------------------------------
    // PtRect::contains
    // ------------------------------------------------------------------

    #[test]
    fn pt_rect_contains_inside() {
        let r = PtRect { left: 0.0, bottom: 0.0, right: 10.0, top: 10.0 };
        assert!(r.contains(5.0, 5.0));
    }

    #[test]
    fn pt_rect_contains_outside() {
        let r = PtRect { left: 0.0, bottom: 0.0, right: 10.0, top: 10.0 };
        assert!(!r.contains(15.0, 5.0));
        assert!(!r.contains(5.0, 15.0));
        assert!(!r.contains(-1.0, 5.0));
    }

    #[test]
    fn pt_rect_contains_on_edge() {
        // Inclusive on edges
        let r = PtRect { left: 0.0, bottom: 0.0, right: 10.0, top: 10.0 };
        assert!(r.contains(0.0, 0.0));
        assert!(r.contains(10.0, 10.0));
        assert!(r.contains(10.0, 5.0));
    }

    // ------------------------------------------------------------------
    // normalize_bounds / bounds_delta_matrix（图片移动/缩放）
    // ------------------------------------------------------------------

    #[test]
    fn normalize_bounds_orders_corners() {
        assert_eq!(normalize_bounds(40.0, 60.0, 10.0, 20.0), (10.0, 20.0, 40.0, 60.0));
        assert_eq!(normalize_bounds(10.0, 20.0, 40.0, 60.0), (10.0, 20.0, 40.0, 60.0));
    }

    /// The delta matrix must map the old AABB's two corners exactly onto the
    /// target rect's corners.
    #[test]
    fn bounds_delta_matrix_maps_corners_to_target() {
        let target = PtRect { left: 100.0, bottom: 200.0, right: 160.0, top: 280.0 };
        let m = bounds_delta_matrix(10.0, 20.0, 40.0, 60.0, &target).expect("non-degenerate");

        let (x0, y0) = m.apply_to_points(PdfPoints::new(10.0), PdfPoints::new(20.0));
        assert!((x0.value - 100.0).abs() < 1e-3, "left-bottom x: {}", x0.value);
        assert!((y0.value - 200.0).abs() < 1e-3, "left-bottom y: {}", y0.value);

        let (x1, y1) = m.apply_to_points(PdfPoints::new(40.0), PdfPoints::new(60.0));
        assert!((x1.value - 160.0).abs() < 1e-3, "right-top x: {}", x1.value);
        assert!((y1.value - 280.0).abs() < 1e-3, "right-top y: {}", y1.value);
    }

    /// Pure translation (same size) keeps scale factors at 1.
    #[test]
    fn bounds_delta_matrix_pure_translation() {
        let target = PtRect { left: 15.0, bottom: 25.0, right: 45.0, top: 65.0 };
        let m = bounds_delta_matrix(10.0, 20.0, 40.0, 60.0, &target).expect("non-degenerate");
        assert!((m.a() - 1.0).abs() < 1e-6);
        assert!((m.d() - 1.0).abs() < 1e-6);
        assert!((m.b()).abs() < 1e-6);
        assert!((m.c()).abs() < 1e-6);
        assert!((m.e() - 5.0).abs() < 1e-6);
        assert!((m.f() - 5.0).abs() < 1e-6);
    }

    /// Degenerate source bounds cannot be scaled and must be rejected.
    #[test]
    fn bounds_delta_matrix_rejects_degenerate_bounds() {
        let target = PtRect { left: 0.0, bottom: 0.0, right: 10.0, top: 10.0 };
        assert!(bounds_delta_matrix(5.0, 0.0, 5.0, 10.0, &target).is_none());
        assert!(bounds_delta_matrix(0.0, 5.0, 10.0, 5.0, &target).is_none());
    }

    /// `m_old * delta` must be equivalent to applying `m_old` and then `delta`,
    /// which is exactly why the delta lands the object's AABB on the target
    /// rect in page space (tested above with the identity matrix).
    #[test]
    fn bounds_delta_matrix_composes_after_old_matrix() {
        let old = PdfMatrix::new(2.0, 1.0, -1.0, 3.0, 7.0, 11.0);
        let target = PtRect { left: 50.0, bottom: 60.0, right: 90.0, top: 100.0 };
        let delta = bounds_delta_matrix(10.0, 20.0, 40.0, 60.0, &target).expect("non-degenerate");
        let composed = old.multiply(delta);

        let p = (PdfPoints::new(3.0), PdfPoints::new(4.0));
        let (cx, cy) = composed.apply_to_points(p.0, p.1);
        let (ox, oy) = old.apply_to_points(p.0, p.1);
        let (dx, dy) = delta.apply_to_points(ox, oy);
        assert!((cx.value - dx.value).abs() < 1e-3);
        assert!((cy.value - dy.value).abs() < 1e-3);
    }

    /// The delta matrix takes page-space points onto the target rect, so
    /// composing it after any object matrix preserves the mapping.
    #[test]
    fn bounds_delta_matrix_maps_page_space_points() {
        let target = PtRect { left: 50.0, bottom: 60.0, right: 90.0, top: 100.0 };
        let delta = bounds_delta_matrix(10.0, 20.0, 40.0, 60.0, &target).expect("non-degenerate");
        let (x0, y0) = delta.apply_to_points(PdfPoints::new(10.0), PdfPoints::new(20.0));
        assert!((x0.value - 50.0).abs() < 1e-3);
        assert!((y0.value - 60.0).abs() < 1e-3);
        let (x1, y1) = delta.apply_to_points(PdfPoints::new(40.0), PdfPoints::new(60.0));
        assert!((x1.value - 90.0).abs() < 1e-3);
        assert!((y1.value - 100.0).abs() < 1e-3);
    }
}
