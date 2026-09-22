//! Watermark Removal (M4 part 2)
//! Manual rectangular erasure + automatic detection.
//! - Manual: delete all page objects within a given rectangle
//! - Automatic: cross-page sampling -> fingerprint hashing -> recurring objects are candidates

use pdfium_render::prelude::*;
use serde::Deserialize;
use tauri::State;

use crate::document::{pdfium as get_pdfium, push_snapshot, AppState, DocumentInfo};
use crate::error::{AppError, AppResult};
use crate::pages::{commit_and_return, load_doc, normalize_indices};

/// Rectangle in PDF points (origin at bottom-left of page).
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl Rect {
    fn width(&self) -> f32 {
        (self.right - self.left).max(0.0)
    }
    fn height(&self) -> f32 {
        (self.top - self.bottom).max(0.0)
    }
    pub fn valid(&self) -> bool {
        self.width() > 0.0 && self.height() > 0.0
    }
}

/// Fingerprint of a single page object (used by detector).
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ObjectFingerprint {
    pub object_index: u32,
    pub kind: String,
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
    pub occurrence: u32,
    pub total_sampled: u32,
}

/// Result of automatic watermark detection.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectResult {
    pub candidates: Vec<ObjectFingerprint>,
    pub total_pages: u32,
    pub sampled_pages: u32,
}

/// Summary returned by removal commands.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedSummary {
    pub info: DocumentInfo,
    pub removed_count: u32,
}

/// AABB intersection test for two rectangles.
pub fn rects_intersect(a: &Rect, b: &Rect) -> bool {
    a.left < b.right && a.right > b.left && a.bottom < b.top && a.top > b.bottom
}

/// Convert a `PdfQuadPoints` (returned by `bounds()`) into our flat `Rect`.
fn rect_from_pdfqp(r: &pdfium_render::prelude::PdfQuadPoints) -> Rect {
    Rect {
        left: r.left().value as f32,
        bottom: r.bottom().value as f32,
        right: r.right().value as f32,
        top: r.top().value as f32,
    }
}

/// Quantize a coordinate to a grid step (for fingerprint matching).
pub fn quantize(v: f32, step: f32) -> i32 {
    (v / step).round() as i32
}

/// Manual rectangular erasure: delete all page objects that intersect the given rectangle
/// on the specified pages.
#[tauri::command]
pub async fn remove_objects_in_rect(
    state: State<'_, AppState>,
    doc_id: u64,
    pages: Vec<u32>,
    rect: Rect,
) -> AppResult<RemovedSummary> {
    if !rect.valid() {
        return Err(AppError::InvalidRect);
    }
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let (new_bytes, removed_total) = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        let total = doc.pages().len() as u32;
        let indices = normalize_indices(&pages, total)?;
        let mut removed_total = 0u32;
        {
            let pages_col = doc.pages_mut();
            for idx in indices {
                let mut page = pages_col.get(idx)?;
                // Collect indices to remove while we still have a read-only borrow.
                let mut to_remove: Vec<u32> = Vec::new();
                {
                    let objs = page.objects();
                    for (i, obj) in objs.iter().enumerate() {
                        let obj_rect = rect_from_pdfqp(&obj.bounds()?);
                        if rects_intersect(&rect, &obj_rect) {
                            to_remove.push(i as u32);
                        }
                    }
                }
                // Remove in reverse order so indices stay valid.
                for &i in to_remove.iter().rev() {
                    if page
                        .objects_mut()
                        .remove_object_at_index(i as usize)
                        .is_ok()
                    {
                        removed_total += 1;
                    }
                }
            }
        }
        let bytes = doc.save_to_bytes()?;
        (bytes, removed_total)
    };
    let info = commit_and_return(&state, doc_id, new_bytes)?;
    Ok(RemovedSummary {
        info,
        removed_count: removed_total,
    })
}

/// Automatic watermark detection: sample multiple pages, hash each object by
/// (type, quantized bounds, fill color), and return objects whose hash recurs
/// in at least `threshold` fraction of the sampled pages.
#[tauri::command]
pub async fn detect_watermark_candidates(
    state: State<'_, AppState>,
    doc_id: u64,
    sample_pages: Option<u32>,
    threshold: Option<f32>,
) -> AppResult<DetectResult> {
    let pdfium = get_pdfium();
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    let doc = load_doc(pdfium, &entry.bytes)?;
    let total = doc.pages().len() as u32;
    if total == 0 {
        return Ok(DetectResult {
            candidates: Vec::new(),
            total_pages: 0,
            sampled_pages: 0,
        });
    }
    let sample_n = sample_pages.unwrap_or(10).clamp(2, 50).min(total);
    let threshold = threshold.unwrap_or(0.6).clamp(0.3, 1.0);

    // Even sampling across the document.
    let sampled_indices: Vec<u32> = if sample_n >= total {
        (0..total).collect()
    } else {
        let step = total as f32 / sample_n as f32;
        (0..sample_n).map(|i| (i as f32 * step) as u32).collect()
    };
    let sampled_pages = sampled_indices.len() as u32;

    use std::collections::HashMap;
    // key: fingerprint hash string -> (occurrence count, first-seen fingerprint)
    let mut fp_counts: HashMap<String, (u32, ObjectFingerprint)> = HashMap::new();

    let pages = doc.pages();
    for &page_idx in &sampled_indices {
        let Ok(page) = pages.get(page_idx as u16) else {
            continue;
        };
        let objs = page.objects();
        for (i, obj) in objs.iter().enumerate() {
            let kind = match obj.object_type() {
                PdfPageObjectType::Text => "text",
                PdfPageObjectType::Image => "image",
                PdfPageObjectType::Path => "path",
                _ => continue,
            };
            let bounds = obj.bounds()?;
            let l = bounds.left().value as f32;
            let b = bounds.bottom().value as f32;
            let r = bounds.right().value as f32;
            let t = bounds.top().value as f32;
            let quant_l = quantize(l, 5.0);
            let quant_b = quantize(b, 5.0);
            let quant_w = quantize(r - l, 5.0);
            let quant_h = quantize(t - b, 5.0);
            let color_hash = match obj.fill_color() {
                Ok(c) => ((c.red() as u32) << 16) | ((c.green() as u32) << 8) | (c.blue() as u32),
                Err(_) => 0,
            };
            let key = format!(
                "{}|{}|{}|{}|{}|{}",
                kind, quant_l, quant_b, quant_w, quant_h, color_hash
            );
            let fp = ObjectFingerprint {
                object_index: i as u32,
                kind: kind.to_string(),
                left: l,
                bottom: b,
                right: r,
                top: t,
                occurrence: 0,
                total_sampled: sampled_pages,
            };
            fp_counts.entry(key).or_insert((0, fp)).0 += 1;
        }
    }

    let mut candidates: Vec<ObjectFingerprint> = Vec::new();
    for (_, (count, mut fp)) in fp_counts {
        let ratio = count as f32 / sampled_pages as f32;
        if ratio >= threshold {
            fp.occurrence = count;
            candidates.push(fp);
        }
    }

    Ok(DetectResult {
        candidates,
        total_pages: total,
        sampled_pages,
    })
}

/// Apply the user-selected removal: delete the chosen object index from every
/// page of the document (regardless of type or fingerprint match).
#[tauri::command]
pub async fn apply_watermark_removal(
    state: State<'_, AppState>,
    doc_id: u64,
    selected_indices: Vec<u32>,
) -> AppResult<RemovedSummary> {
    if selected_indices.is_empty() {
        return Err(AppError::NoCandidates);
    }
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let (new_bytes, removed_total) = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        let total = doc.pages().len() as u32;
        let pages_col = doc.pages_mut();
        let mut removed_total = 0u32;
        for idx in 0..total {
            let mut page = pages_col.get(idx as u16)?;
            let obj_count = page.objects().len() as u32;
            for &sel in selected_indices.iter().rev() {
                if sel < obj_count
                    && page
                        .objects_mut()
                        .remove_object_at_index(sel as usize)
                        .is_ok()
                {
                    removed_total += 1;
                }
            }
        }
        let bytes = doc.save_to_bytes()?;
        (bytes, removed_total)
    };
    let info = commit_and_return(&state, doc_id, new_bytes)?;
    Ok(RemovedSummary {
        info,
        removed_count: removed_total,
    })
}