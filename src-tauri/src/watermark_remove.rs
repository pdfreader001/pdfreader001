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
///
/// `key` is the internal quantized hash; the frontend never needs to read it
/// but can echo it back to `apply_watermark_removal` so removal matches the
/// exact same object across pages (object index alone is not stable).
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
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

/// Compact text representation used as part of a fingerprint. Trims ASCII
/// whitespace and lowercases ASCII letters so trivial casing/spacing changes
/// do not produce a different key.
fn normalize_text_for_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
            continue;
        }
        prev_space = false;
        for lc in ch.to_lowercase() {
            out.push(lc);
        }
    }
    out.trim().to_string()
}

/// Compute the (kind, quantized bounds, fill color, optional text) key for
/// a page object. Exposed so the removal command can rebuild the same key
/// per page.
fn fingerprint_key(obj: &PdfPageObject) -> Option<(String, String)> {
    let kind = match obj.object_type() {
        PdfPageObjectType::Text => "text",
        PdfPageObjectType::Image => "image",
        PdfPageObjectType::Path => "path",
        _ => return None,
    }
    .to_string();
    let bounds = obj.bounds().ok()?;
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
    let text_part = if obj.object_type() == PdfPageObjectType::Text {
        obj.as_text_object()
            .map(|t| normalize_text_for_key(&t.text()))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let key = format!(
        "{}|{}|{}|{}|{}|{}|{}",
        kind, quant_l, quant_b, quant_w, quant_h, color_hash, text_part
    );
    Some((kind, key))
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
            let (kind, key) = match fingerprint_key(&obj) {
                Some(pair) => pair,
                None => continue,
            };
            let bounds = obj.bounds()?;
            let l = bounds.left().value as f32;
            let b = bounds.bottom().value as f32;
            let r = bounds.right().value as f32;
            let t = bounds.top().value as f32;
            let fp = ObjectFingerprint {
                object_index: i as u32,
                kind,
                left: l,
                bottom: b,
                right: r,
                top: t,
                occurrence: 0,
                total_sampled: sampled_pages,
                key: Some(key.clone()),
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

/// Apply the user-selected removal. `fingerprint_keys` is the preferred input:
/// each key identifies a recurring watermark object detected by
/// `detect_watermark_candidates` and is matched on every page. `selected_indices`
/// is a legacy fallback that interprets the values as raw object indices and
/// is only reliable within a single page; keep it for backward compatibility.
#[tauri::command]
pub async fn apply_watermark_removal(
    state: State<'_, AppState>,
    doc_id: u64,
    fingerprint_keys: Option<Vec<String>>,
    selected_indices: Option<Vec<u32>>,
) -> AppResult<RemovedSummary> {
    let keys = fingerprint_keys.unwrap_or_default();
    let legacy = selected_indices.unwrap_or_default();
    if keys.is_empty() && legacy.is_empty() {
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
            // Collect indices to remove (highest first), then remove in one
            // pass. We must drop the read-only `objects()` borrow before
            // calling `objects_mut()`.
            let mut to_remove: Vec<u32> = Vec::new();
            {
                let objs = page.objects();
                if !keys.is_empty() {
                    for (i, obj) in objs.iter().enumerate() {
                        let Some((_, k)) = fingerprint_key(&obj) else {
                            continue;
                        };
                        if keys.iter().any(|target| target == &k) {
                            to_remove.push(i as u32);
                        }
                    }
                } else {
                    // Legacy path: treat the supplied indices as positional.
                    let obj_count = objs.len() as u32;
                    for &sel in &legacy {
                        if sel < obj_count {
                            to_remove.push(sel);
                        }
                    }
                }
            }
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
        let bytes = doc.save_to_bytes()?;
        (bytes, removed_total)
    };
    let info = commit_and_return(&state, doc_id, new_bytes)?;
    Ok(RemovedSummary {
        info,
        removed_count: removed_total,
    })
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
    // Rect::width / height / valid
    // ------------------------------------------------------------------

    #[test]
    fn test_rect_width_positive() {
        let r = Rect { left: 10.0, bottom: 0.0, right: 50.0, top: 0.0 };
        assert!(approx(r.width(), 40.0));
    }

    #[test]
    fn test_rect_width_negative_clamps_zero() {
        let r = Rect { left: 50.0, bottom: 0.0, right: 10.0, top: 0.0 };
        assert!(approx(r.width(), 0.0));
    }

    #[test]
    fn test_rect_height_positive() {
        let r = Rect { left: 0.0, bottom: 10.0, right: 0.0, top: 50.0 };
        assert!(approx(r.height(), 40.0));
    }

    #[test]
    fn test_rect_height_negative_clamps_zero() {
        let r = Rect { left: 0.0, bottom: 50.0, right: 0.0, top: 10.0 };
        assert!(approx(r.height(), 0.0));
    }

    #[test]
    fn test_rect_valid_true() {
        let r = Rect { left: 0.0, bottom: 0.0, right: 10.0, top: 10.0 };
        assert!(r.valid());
    }

    #[test]
    fn test_rect_valid_zero_width() {
        let r = Rect { left: 5.0, bottom: 0.0, right: 5.0, top: 10.0 };
        assert!(!r.valid());
    }

    #[test]
    fn test_rect_valid_zero_height() {
        let r = Rect { left: 0.0, bottom: 5.0, right: 10.0, top: 5.0 };
        assert!(!r.valid());
    }

    #[test]
    fn test_rect_valid_inverted() {
        let r = Rect { left: 10.0, bottom: 10.0, right: 0.0, top: 0.0 };
        assert!(!r.valid());
    }

    // ------------------------------------------------------------------
    // rects_intersect
    // ------------------------------------------------------------------

    #[test]
    fn test_rects_intersect_overlap() {
        let a = Rect { left: 0.0, bottom: 0.0, right: 10.0, top: 10.0 };
        let b = Rect { left: 5.0, bottom: 5.0, right: 15.0, top: 15.0 };
        assert!(rects_intersect(&a, &b));
        assert!(rects_intersect(&b, &a));
    }

    #[test]
    fn test_rects_intersect_fully_contained() {
        let a = Rect { left: 0.0, bottom: 0.0, right: 20.0, top: 20.0 };
        let b = Rect { left: 5.0, bottom: 5.0, right: 15.0, top: 15.0 };
        assert!(rects_intersect(&a, &b));
    }

    #[test]
    fn test_rects_intersect_separate_x() {
        let a = Rect { left: 0.0, bottom: 0.0, right: 5.0, top: 10.0 };
        let b = Rect { left: 10.0, bottom: 0.0, right: 15.0, top: 10.0 };
        assert!(!rects_intersect(&a, &b));
    }

    #[test]
    fn test_rects_intersect_separate_y() {
        let a = Rect { left: 0.0, bottom: 0.0, right: 10.0, top: 5.0 };
        let b = Rect { left: 0.0, bottom: 10.0, right: 10.0, top: 15.0 };
        assert!(!rects_intersect(&a, &b));
    }

    #[test]
    fn test_rects_intersect_touch_not_overlap() {
        // Touching at an edge — AABB intersection uses strict inequality
        let a = Rect { left: 0.0, bottom: 0.0, right: 10.0, top: 10.0 };
        let b = Rect { left: 10.0, bottom: 0.0, right: 20.0, top: 10.0 };
        assert!(!rects_intersect(&a, &b));
    }

    #[test]
    fn test_rects_intersect_identical() {
        let a = Rect { left: 1.0, bottom: 2.0, right: 3.0, top: 4.0 };
        assert!(rects_intersect(&a, &a));
    }

    // ------------------------------------------------------------------
    // quantize
    // ------------------------------------------------------------------

    #[test]
    fn test_quantize_exact_multiple() {
        assert_eq!(quantize(10.0, 5.0), 2);
        assert_eq!(quantize(0.0, 5.0), 0);
        assert_eq!(quantize(-10.0, 5.0), -2);
    }

    #[test]
    fn test_quantize_rounds_half_away_from_zero() {
        // Rust f32::round() uses round half away from zero
        assert_eq!(quantize(7.5, 5.0), 2); // 1.5 -> 2
        assert_eq!(quantize(-7.5, 5.0), -2); // -1.5 -> -2
    }

    #[test]
    fn test_quantize_rounds_half_below() {
        assert_eq!(quantize(7.0, 5.0), 1); // 1.4 -> 1
        assert_eq!(quantize(9.0, 5.0), 2); // 1.8 -> 2
    }

    #[test]
    fn test_quantize_step_one() {
        assert_eq!(quantize(3.7, 1.0), 4);
        assert_eq!(quantize(3.2, 1.0), 3);
    }

    // ------------------------------------------------------------------
    // normalize_text_for_key
    // ------------------------------------------------------------------

    #[test]
    fn test_normalize_text_collapses_whitespace() {
        assert_eq!(normalize_text_for_key("  Hello\t World  "), "hello world");
    }

    #[test]
    fn test_normalize_text_lowercases_ascii() {
        assert_eq!(normalize_text_for_key("CONFIDENTIAL"), "confidential");
    }

    #[test]
    fn test_normalize_text_trims_and_no_internal_dup_space() {
        assert_eq!(normalize_text_for_key("A   B\n\nC"), "a b c");
    }

    #[test]
    fn test_normalize_text_keeps_non_ascii_unchanged() {
        // CJK characters pass through (to_lowercase is a no-op on them)
        assert_eq!(normalize_text_for_key("机密文件"), "机密文件");
    }

    #[test]
    fn test_normalize_text_empty() {
        assert_eq!(normalize_text_for_key("   \t\n  "), "");
    }
}