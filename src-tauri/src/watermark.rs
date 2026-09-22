//! M4：水印（文字 / 图片），支持位置、透明度、旋转与平铺。
//!
//! 透明度实现：
//! - 文字水印 → 填充色带 alpha（PdfColor 第四通道）
//! - 图片水印 → 像素级预乘 alpha 后再嵌入

use std::path::Path;

use pdfium_render::prelude::*;
use serde::Deserialize;
use tauri::State;

use crate::document::{pdfium as get_pdfium, push_snapshot, AppState, DocumentInfo};
use crate::error::{AppError, AppResult};
use crate::pages::{commit_and_return, load_doc, normalize_indices};

/// 九宫格位置锚点标识（前端字符串原样传入）
const MARGIN: f32 = 36.0;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatermarkStyle {
    /// 0–100
    pub opacity: f32,
    /// 顺时针角度
    pub rotation: f32,
    /// "top-left" … "center" … "bottom-right"
    pub position: String,
    pub tiled: bool,
    /// 平铺间距（pt）
    pub tile_spacing: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextWatermarkOpts {
    pub text: String,
    /// 字号（pt）
    pub font_size: f32,
    /// "#RRGGBB"
    pub color: String,
    pub style: WatermarkStyle,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageWatermarkOpts {
    pub image_path: String,
    /// 水印宽度占页面宽度的比例（0.05–1.0）
    pub scale: f32,
    pub style: WatermarkStyle,
}

// ---------- 工具函数 ----------

fn parse_color(hex: &str, opacity: f32) -> PdfColor {
    let h = hex.trim_start_matches('#');
    let byte = |s: &str| u8::from_str_radix(s, 16).unwrap_or(128);
    let (r, g, b) = match h.len() {
        6 => (byte(&h[0..2]), byte(&h[2..4]), byte(&h[4..6])),
        _ => (128, 128, 128),
    };
    let a = (opacity.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8;
    PdfColor::new(r, g, b, a)
}

/// 估算文本宽度：CJK 字符按全宽、其余按 0.55 倍字号
fn estimate_text_width(text: &str, font_size: f32) -> f32 {
    text.chars()
        .map(|c| if (c as u32) >= 0x2E80 { 1.0 } else { 0.55 })
        .sum::<f32>()
        * font_size
}

/// 九宫格锚点 → 对象左下角坐标
fn anchor(position: &str, page_w: f32, page_h: f32, obj_w: f32, obj_h: f32) -> (f32, f32) {
    let (fx, fy) = match position {
        "top-left" => (0.0, 1.0),
        "top-center" => (0.5, 1.0),
        "top-right" => (1.0, 1.0),
        "middle-left" => (0.0, 0.5),
        "middle-right" => (1.0, 0.5),
        "bottom-left" => (0.0, 0.0),
        "bottom-center" => (0.5, 0.0),
        "bottom-right" => (1.0, 0.0),
        _ => (0.5, 0.5), // center
    };
    let x = MARGIN + fx * (page_w - 2.0 * MARGIN - obj_w);
    let y = MARGIN + fy * (page_h - 2.0 * MARGIN - obj_h);
    (x.max(0.0), y.max(0.0))
}

/// 按需加载字体：纯 ASCII 用内置 Helvetica；含中文则尝试系统中文字体（CID 加载）
pub(crate) fn load_font_for_text(doc: &mut PdfDocument, text: &str) -> AppResult<PdfFontToken> {
    if text.chars().all(|c| c.is_ascii()) {
        return Ok(doc.fonts_mut().helvetica());
    }
    const CANDIDATES: [&str; 4] = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\msyhbd.ttc",
    ];
    for path in CANDIDATES {
        if !Path::new(path).exists() {
            continue;
        }
        // 先按 CID 键控（微软字体常见），失败再按普通 TrueType
        if let Ok(token) = doc.fonts_mut().load_true_type_from_file(path, true) {
            return Ok(token);
        }
        if let Ok(token) = doc.fonts_mut().load_true_type_from_file(path, false) {
            return Ok(token);
        }
    }
    Err(AppError::NoChineseFont)
}

/// 平铺网格生成器：交错排列覆盖整页
fn tile_positions(
    page_w: f32,
    page_h: f32,
    obj_w: f32,
    obj_h: f32,
    spacing: f32,
) -> Vec<(f32, f32)> {
    let spacing = if spacing > 10.0 { spacing } else { 120.0 };
    let step_x = obj_w + spacing;
    let step_y = obj_h + spacing;
    let mut out = Vec::new();
    let mut row = 0u32;
    let mut y = -step_y;
    while y < page_h {
        let offset = (row % 2) as f32 * step_x / 2.0;
        let mut x = -step_x + offset;
        while x < page_w {
            out.push((x, y));
            x += step_x;
        }
        y += step_y;
        row += 1;
    }
    out
}

/// 图片水印透明度：像素级乘 alpha
fn apply_image_opacity(mut img: image::DynamicImage, opacity: f32) -> image::DynamicImage {
    let factor = opacity.clamp(0.0, 100.0) / 100.0;
    if factor >= 0.999 {
        return img;
    }
    let rgba = img.to_rgba8();
    let mut rgba = rgba;
    for p in rgba.pixels_mut() {
        p[3] = (p[3] as f32 * factor).round() as u8;
    }
    image::DynamicImage::ImageRgba8(rgba)
}

// ---------- 命令 ----------

#[tauri::command]
pub async fn add_text_watermark(
    state: State<'_, AppState>,
    doc_id: u64,
    pages: Vec<u32>,
    opts: TextWatermarkOpts,
) -> AppResult<DocumentInfo> {
    if opts.text.trim().is_empty() {
        return Err(AppError::WatermarkTextEmpty);
    }
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        let total = doc.pages().len() as u32;
        let indices = normalize_indices(&pages, total)?;
        // 先取字体 token，避免与 pages_mut 可变借用冲突
        let font_token = load_font_for_text(&mut doc, &opts.text)?;
        let color = parse_color(&opts.color, opts.style.opacity);
        let rot = opts.style.rotation;
        let tiled = opts.style.tiled;
        let spacing = opts.style.tile_spacing;
        let font_size = opts.font_size.max(4.0);
        let text_w = estimate_text_width(&opts.text, font_size);
        {
            let pages_col = doc.pages_mut();
            for idx in indices {
                let mut page = pages_col.get(idx)?;
                let pw = page.width().value as f32;
                let ph = page.height().value as f32;
                let spots = if tiled {
                    tile_positions(pw, ph, text_w, font_size, spacing)
                } else {
                    vec![anchor(&opts.style.position, pw, ph, text_w, font_size)]
                };
                let objects = page.objects_mut();
                for (x, y) in spots {
                    let mut obj = objects.create_text_object(
                        PdfPoints::new(x as f32),
                        PdfPoints::new(y as f32),
                        &opts.text,
                        font_token,
                        PdfPoints::new(font_size),
                    )?;
                    obj.set_fill_color(color)?;
                    if rot.abs() > 0.01 {
                        obj.rotate_clockwise_degrees(rot)?;
                    }
                }
            }
        }
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

#[tauri::command]
pub async fn add_image_watermark(
    state: State<'_, AppState>,
    doc_id: u64,
    pages: Vec<u32>,
    opts: ImageWatermarkOpts,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let img = image::open(&opts.image_path)
        .map_err(|_| AppError::ImageReadFailed { path: opts.image_path.clone() })?;
    let img = apply_image_opacity(img, opts.style.opacity);
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    if iw < 1.0 || ih < 1.0 {
        return Err(AppError::InvalidImageSize);
    }
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        let total = doc.pages().len() as u32;
        let indices = normalize_indices(&pages, total)?;
        let rot = opts.style.rotation;
        let tiled = opts.style.tiled;
        let spacing = opts.style.tile_spacing;
        let scale = opts.scale.clamp(0.05, 1.0);
        {
            let pages_col = doc.pages_mut();
            for idx in indices {
                let mut page = pages_col.get(idx)?;
                let pw = page.width().value as f32;
                let ph = page.height().value as f32;
                // 水印宽度 = 页宽 × scale，高度按纵横比
                let wm_w = pw * scale;
                let wm_h = wm_w * ih / iw;
                let spots = if tiled {
                    tile_positions(pw, ph, wm_w, wm_h, spacing)
                } else {
                    vec![anchor(&opts.style.position, pw, ph, wm_w, wm_h)]
                };
                let objects = page.objects_mut();
                for (x, y) in spots {
                    let mut obj = objects.create_image_object(
                        PdfPoints::new(x),
                        PdfPoints::new(y),
                        &img,
                        Some(PdfPoints::new(wm_w)),
                        Some(PdfPoints::new(wm_h)),
                    )?;
                    if rot.abs() > 0.01 {
                        obj.rotate_clockwise_degrees(rot)?;
                    }
                }
            }
        }
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}
