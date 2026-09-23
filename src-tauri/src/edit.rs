//! M5：PDF 注释（annotations）管理
//!
//! 支持：高亮 / 下划线 / 删除线 / 便签 / 自由文本 / 矩形
//! 支持：列出注释、删除注释、清空页面注释
//!
//! 坐标约定：前端传归一化比例（0–1，CSS 左上原点），后端换算为 PDF 点（pt，左下原点）。

use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::document::{pdfium as get_pdfium, push_snapshot, AppState, DocumentInfo};
use crate::error::{AppError, AppResult};
use crate::pages::{commit_and_return, load_doc, normalize_indices};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnnotationKind {
    Highlight,
    Underline,
    Strikeout,
    StickyNote,
    FreeText,
    Square,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationInfo {
    pub index: u32,
    pub page_index: u32,
    pub kind: AnnotationKind,
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
    pub contents: String,
    pub color: String,
}

/// 归一化区域（0–1，CSS 坐标系：左上原点）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionSpec {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddAnnotationOpts {
    pub kind: AnnotationKind,
    pub region: RegionSpec,
    pub contents: String,
    /// "#RRGGBB"
    pub color: String,
    pub opacity: f32,
}

fn parse_color_hex(hex: &str) -> PdfColor {
    let h = hex.trim_start_matches('#');
    let byte = |s: &str| u8::from_str_radix(s, 16).unwrap_or(255);
    match h.len() {
        6 => PdfColor::new(byte(&h[0..2]), byte(&h[2..4]), byte(&h[4..6]), 255),
        _ => PdfColor::new(255, 255, 0, 255),
    }
}

fn color_to_hex(c: PdfColor) -> String {
    format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue())
}

/// 归一化区域 → PDF 坐标 (left, bottom, right, top)
fn region_to_pdf_rect(page_w: f32, page_h: f32, r: &RegionSpec) -> (PdfPoints, PdfPoints, PdfPoints, PdfPoints) {
    let l = (r.left.clamp(0.0, 1.0) * page_w).max(0.0);
    let r_x = ((r.left + r.width).clamp(0.0, 1.0) * page_w).max(0.0);
    // CSS top → PDF：y 从顶向下，PDF y 从底向上
    let t = ((1.0 - r.top.clamp(0.0, 1.0)) * page_h).max(0.0);
    let b = ((1.0 - (r.top + r.height).clamp(0.0, 1.0)) * page_h).max(0.0);
    let left = PdfPoints::new(l.min(r_x));
    let right = PdfPoints::new(l.max(r_x));
    let bottom = PdfPoints::new(t.min(b));
    let top = PdfPoints::new(t.max(b));
    (left, bottom, right, top)
}

fn make_rect(page_w: f32, page_h: f32, r: &RegionSpec) -> PdfRect {
    let (left, bottom, right, top) = region_to_pdf_rect(page_w, page_h, r);
    PdfRect::new(bottom, left, top, right)
}

fn make_quad(page_w: f32, page_h: f32, r: &RegionSpec) -> PdfQuadPoints {
    let (left, bottom, right, top) = region_to_pdf_rect(page_w, page_h, r);
    // 左上 → 右上 → 右下 → 左下（PDF 坐标）
    PdfQuadPoints::new(left, top, right, top, right, bottom, left, bottom)
}

fn annotation_to_color(annot: &pdfium_render::prelude::PdfPageAnnotation<'_>) -> String {
    if let Ok(c) = annot.stroke_color() {
        color_to_hex(c)
    } else if let Ok(c) = annot.fill_color() {
        color_to_hex(c)
    } else {
        "#ffff00".to_string()
    }
}

fn map_kind(kind: PdfPageAnnotationType) -> Option<AnnotationKind> {
    match kind {
        PdfPageAnnotationType::Highlight => Some(AnnotationKind::Highlight),
        PdfPageAnnotationType::Underline => Some(AnnotationKind::Underline),
        PdfPageAnnotationType::Strikeout => Some(AnnotationKind::Strikeout),
        PdfPageAnnotationType::Text => Some(AnnotationKind::StickyNote),
        PdfPageAnnotationType::FreeText => Some(AnnotationKind::FreeText),
        PdfPageAnnotationType::Square => Some(AnnotationKind::Square),
        _ => None,
    }
}

// ---------- 纯函数（可测） ----------

/// 列出页面注释（纯函数版本）。
/// page_index = None 表示所有页。
pub fn list_annotations_logic(bytes: &[u8], page_index: Option<u32>) -> AppResult<Vec<AnnotationInfo>> {
    let pdfium = get_pdfium();
    let doc = load_doc(pdfium, bytes)?;
    let total = doc.pages().len() as u32;
    let range: Vec<u32> = match page_index {
        Some(p) if p < total => vec![p],
        Some(_) => return Err(AppError::PageOutOfRange),
        None => (0..total).collect(),
    };
    let mut out = Vec::new();
    let pages = doc.pages();
    for p_idx in range {
        let page = pages.get(p_idx as u16)?;
        let annots = page.annotations();
        let count = annots.len() as u32;
        for i in 0..count {
            let annot = match annots.get(i as usize) {
                Ok(a) => a,
                Err(_) => continue,
            };
            let Some(kind) = map_kind(annot.annotation_type()) else {
                continue;
            };
            let bounds = match annot.bounds() {
                Ok(b) => b,
                Err(_) => continue,
            };
            out.push(AnnotationInfo {
                index: i,
                page_index: p_idx,
                kind,
                left: bounds.left.value as f32,
                bottom: bounds.bottom.value as f32,
                right: bounds.right.value as f32,
                top: bounds.top.value as f32,
                contents: annot.contents().unwrap_or_default(),
                color: annotation_to_color(&annot),
            });
        }
    }
    Ok(out)
}

/// 添加注释（纯函数版本）：返回新 bytes。
pub fn add_annotation_logic(
    bytes: &[u8],
    page_index: u32,
    opts: &AddAnnotationOpts,
) -> AppResult<Vec<u8>> {
    let pdfium = get_pdfium();
    let mut doc = load_doc(pdfium, bytes)?;
    let total = doc.pages().len() as u32;
    if page_index >= total {
        return Err(AppError::PageOutOfRange);
    }
    let color = parse_color_hex(&opts.color);
    {
        let mut pages = doc.pages_mut();
        let mut page = pages.get(page_index as u16)?;
        let pw = page.width().value as f32;
        let ph = page.height().value as f32;
        let rect = make_rect(pw, ph, &opts.region);
        let mut annots = page.annotations_mut();
        match opts.kind {
            AnnotationKind::Highlight => {
                let mut annot = annots.create_highlight_annotation()?;
                let q = make_quad(pw, ph, &opts.region);
                annot.attachment_points_mut().create_attachment_point_at_end(q)?;
                annot.set_contents(&opts.contents)?;
                let _ = annot.set_fill_color(color);
                let _ = annot.set_bounds(rect);
            }
            AnnotationKind::Underline => {
                let mut annot = annots.create_underline_annotation()?;
                let q = make_quad(pw, ph, &opts.region);
                annot.attachment_points_mut().create_attachment_point_at_end(q)?;
                annot.set_contents(&opts.contents)?;
                let _ = annot.set_stroke_color(color);
            }
            AnnotationKind::Strikeout => {
                let mut annot = annots.create_strikeout_annotation()?;
                let q = make_quad(pw, ph, &opts.region);
                annot.attachment_points_mut().create_attachment_point_at_end(q)?;
                annot.set_contents(&opts.contents)?;
                let _ = annot.set_stroke_color(color);
            }
            AnnotationKind::StickyNote => {
                let mut annot = annots.create_text_annotation(&opts.contents)?;
                let _ = annot.set_bounds(rect);
                let _ = annot.set_fill_color(color);
            }
            AnnotationKind::FreeText => {
                let mut annot = annots.create_free_text_annotation(&opts.contents)?;
                let _ = annot.set_bounds(rect);
                let _ = annot.set_stroke_color(color);
                let _ = annot.set_fill_color(PdfColor::new(0, 0, 0, 0));
            }
            AnnotationKind::Square => {
                let mut annot = annots.create_square_annotation()?;
                let _ = annot.set_bounds(rect);
                let _ = annot.set_stroke_color(color);
                annot.set_contents(&opts.contents)?;
            }
        }
    }
    Ok(doc.save_to_bytes()?)
}

/// 删除注释（纯函数版本）：返回新 bytes。
pub fn delete_annotation_logic(
    bytes: &[u8],
    page_index: u32,
    annotation_index: u32,
) -> AppResult<Vec<u8>> {
    let pdfium = get_pdfium();
    let mut doc = load_doc(pdfium, bytes)?;
    let total = doc.pages().len() as u32;
    if page_index >= total {
        return Err(AppError::PageOutOfRange);
    }
    {
        let mut pages = doc.pages_mut();
        let mut page = pages.get(page_index as u16)?;
        let mut annots = page.annotations_mut();
        if annotation_index >= annots.len() as u32 {
            return Err(AppError::AnnotationOutOfRange);
        }
        let annot = annots.get(annotation_index as usize)?;
        annots.delete_annotation(annot)?;
    }
    Ok(doc.save_to_bytes()?)
}

// ---------- 命令 ----------

#[tauri::command]
pub async fn list_annotations(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: Option<u32>,
) -> AppResult<Vec<AnnotationInfo>> {
    let pdfium = get_pdfium();
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    list_annotations_logic(&entry.bytes, page_index)
}

#[tauri::command]
pub async fn add_annotation(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
    opts: AddAnnotationOpts,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        add_annotation_logic(&entry.bytes, page_index, &opts)?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

#[tauri::command]
pub async fn delete_annotation(
    state: State<'_, AppState>,
    doc_id: u64,
    page_index: u32,
    annotation_index: u32,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        delete_annotation_logic(&entry.bytes, page_index, annotation_index)?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

#[tauri::command]
pub async fn clear_annotations(
    state: State<'_, AppState>,
    doc_id: u64,
    pages: Vec<u32>,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        let total = doc.pages().len() as u32;
        let indices = normalize_indices(&pages, total)?;
        {
            let mut pages_col = doc.pages_mut();
            for idx in indices.iter().rev() {
                let mut page = pages_col.get(*idx)?;
                let mut annots = page.annotations_mut();
                let n = annots.len();
                for _ in 0..n {
                    if let Ok(annot) = annots.get(0) {
                        let _ = annots.delete_annotation(annot);
                    } else {
                        break;
                    }
                }
            }
        }
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}
