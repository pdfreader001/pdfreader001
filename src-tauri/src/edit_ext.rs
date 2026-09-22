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
use serde::Deserialize;
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
    fn valid(&self) -> bool {
        self.right > self.left && self.top > self.bottom
    }
    fn width(&self) -> f32 {
        (self.right - self.left).max(0.0)
    }
    fn height(&self) -> f32 {
        (self.top - self.bottom).max(0.0)
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

/// 文字重写：在指定页面对 region 内的所有文字对象做"白色覆盖 + 删除 + 插入新文字"。
/// 自动用系统中文字体兜底。
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

/// 文字重写核心逻辑。
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
        let mut doc = load_doc(pdfium, &entry.bytes)?;
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
                    let obj_left = l.min(r);
                    let obj_right = l.max(r);
                    let obj_bottom = b.min(t);
                    let obj_top = b.max(t);
                    // AABB 相交检测
                    if obj_left < region.right
                        && obj_right > region.left
                        && obj_bottom < region.top
                        && obj_top > region.bottom
                    {
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

            // 3. 在 region 底部插入新文字（基线略高于 region.bottom）
            // 用区域宽度的 10% 作为水平 padding
            let pad_x = region.width() * 0.05;
            let text_x = region.left + pad_x;
            // 视觉上把文字放在白色矩形内，偏上一些
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
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

/// 新增文本框：在指定页面的指定坐标插入文本对象（不影响现有对象）。
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
        let mut doc = load_doc(pdfium, &entry.bytes)?;
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
        doc.save_to_bytes()?
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