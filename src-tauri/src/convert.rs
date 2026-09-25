//! M7：常见格式互转（纯本地，零外部依赖）
//!
//! - PDF → PNG / JPEG（按页输出，DPI 可调）
//! - 图片 → PDF（PNG / JPG / JPEG / BMP / WebP → 单页或多页 PDF）
//!
//! 不涉及 Office 文档（Word/Excel/PPT），需要更重量级的库或 LibreOffice。

use std::path::Path;

use image::DynamicImage;
use pdfium_render::prelude::*;
use serde::Deserialize;
use tauri::State;

use crate::document::{pdfium as get_pdfium, AppState};
use crate::error::{AppError, AppResult};
use crate::pages::load_doc;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRangeSpec {
    pub pages: Vec<u32>,
    pub dpi: f64,
    pub format: String, // "png" / "jpeg"
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageToPdfOpts {
    pub image_paths: Vec<String>,
    /// "fit"（按图片实际尺寸）/ "a4" / "letter" / "auto"（取图片中最大者）
    pub page_size: String,
    /// "fill"（铺满）+ "fit"（保持比例居中）
    pub layout: String,
}

/// 纯函数：把一页渲染为指定 DPI 和格式的图片字节。
/// format: "png" / "jpeg" / "jpg"（大小写不敏感），未知格式回退到 PNG。
pub fn export_page_to_image_bytes(
    page: &PdfPage<'_>,
    dpi: f64,
    format: &str,
) -> AppResult<Vec<u8>> {
    let dpi = dpi.clamp(36.0, 600.0);
    let scale = dpi as f32 / 72.0;
    let width = ((page.width().value * scale).round() as i32).max(1);
    let height = ((page.height().value * scale).round() as i32).max(1);
    let bitmap = page.render(width, height, None)?;
    let rgba = bitmap.as_rgba_bytes();
    let w = bitmap.width() as u32;
    let h = bitmap.height() as u32;
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec())
        .ok_or(AppError::ImageConstructFailed)?;
    let dyn_img = image::DynamicImage::ImageRgba8(img);

    let mut out: Vec<u8> = Vec::new();
    match format.to_lowercase().as_str() {
        "jpeg" | "jpg" => {
            dyn_img
                .to_rgb8()
                .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Jpeg)
                .map_err(|e| AppError::Internal(format!("JPEG 编码失败：{e}")))?;
        }
        _ => {
            dyn_img
                .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
                .map_err(|e| AppError::Internal(format!("PNG 编码失败：{e}")))?;
        }
    }
    Ok(out)
}

/// 图片 → PDF 的纯函数选项（与 ImageToPdfOpts 字段相同）。
#[derive(Debug, Clone)]
pub struct ImageToPdfOptions {
    pub page_size: String,
    pub layout: String,
}

/// 纯函数：把一组 DynamicImage 生成 PDF bytes。
pub fn images_to_pdf_from_images(
    images: &[DynamicImage],
    opts: &ImageToPdfOptions,
) -> AppResult<Vec<u8>> {
    if images.is_empty() {
        return Err(AppError::NoImagesProvided);
    }
    let sizes: Vec<(u32, u32)> = images.iter().map(|im| (im.width(), im.height())).collect();

    // 决定每页尺寸（PDF 点）
    let page_size_pts: Vec<(f32, f32)> = match opts.page_size.to_lowercase().as_str() {
        "a4" => vec![(595.0, 842.0); images.len()],
        "letter" => vec![(612.0, 792.0); images.len()],
        "auto" => {
            let mw = sizes.iter().map(|(w, _)| *w).max().unwrap_or(595) as f32;
            let mh = sizes.iter().map(|(_, h)| *h).max().unwrap_or(842) as f32;
            vec![(mw, mh); images.len()]
        }
        _ => sizes
            .iter()
            .map(|(w, h)| (*w as f32, *h as f32))
            .collect(), // fit
    };

    let pdfium = get_pdfium();
    let mut doc = pdfium.create_new_pdf()?;
    {
        let pages = doc.pages_mut();
        for (idx, img) in images.iter().enumerate() {
            let (pw, ph) = page_size_pts[idx];
            let mut page = pages.create_page_at_index(
                PdfPagePaperSize::Custom(PdfPoints::new(pw), PdfPoints::new(ph)),
                idx as u16,
            )?;
            let obj_w = img.width() as f32;
            let obj_h = img.height() as f32;
            let (img_w, img_h) = if opts.layout == "fill" {
                (pw, ph)
            } else {
                let sx = pw / obj_w;
                let sy = ph / obj_h;
                let s = sx.min(sy);
                (obj_w * s, obj_h * s)
            };
            let x = (pw - img_w) / 2.0;
            let y = (ph - img_h) / 2.0;
            page.objects_mut().create_image_object(
                PdfPoints::new(x),
                PdfPoints::new(y),
                img,
                Some(PdfPoints::new(img_w.max(1.0))),
                Some(PdfPoints::new(img_h.max(1.0))),
            )?;
        }
    }
    let bytes = doc.save_to_bytes()?;
    Ok(bytes)
}

/// PDF → PNG/JPEG：逐页渲染为指定 DPI 的位图，写到 output_dir。
/// 返回生成的文件路径列表。
#[tauri::command]
pub async fn export_pages_to_images(
    state: State<'_, AppState>,
    doc_id: u64,
    spec: PageRangeSpec,
    output_dir: String,
) -> AppResult<Vec<String>> {
    if spec.pages.is_empty() {
        return Err(AppError::NoPagesToExport);
    }
    let dir = Path::new(&output_dir);
    if !dir.is_dir() {
        return Err(AppError::Internal(format!(
            "输出目录不存在：{}",
            output_dir
        )));
    }
    let dpi = spec.dpi.clamp(36.0, 600.0);
    // 72 PDF 点 = 1 英寸；dpi → scale
    let scale = dpi as f32 / 72.0;

    let _gate = crate::document::pdfium_gate();
    let pdfium = get_pdfium();
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    let doc = load_doc(pdfium, &entry.bytes)?;
    let total = doc.pages().len() as u32;
    let pages = doc.pages();
    let mut out = Vec::with_capacity(spec.pages.len());

    for (i, page_index) in spec.pages.iter().enumerate() {
        if *page_index >= total {
            return Err(AppError::PageOutOfRange);
        }
        let page = pages.get(*page_index as u16)?;
        let width = ((page.width().value * scale).round() as i32).max(1);
        let height = ((page.height().value * scale).round() as i32).max(1);
        let bitmap = page.render(width, height, None)?;
        let rgba = bitmap.as_rgba_bytes();
        let w = bitmap.width() as u32;
        let h = bitmap.height() as u32;
        // 构造 image::RgbaImage
        let img = image::RgbaImage::from_raw(w, h, rgba.to_vec())
            .ok_or(AppError::ImageConstructFailed)?;
        let dyn_img = image::DynamicImage::ImageRgba8(img);

        let stem = format!("page_{:04}", page_index + 1);
        let path = match spec.format.to_lowercase().as_str() {
            "jpeg" | "jpg" => {
                // JPEG 不支持 alpha，转换为 RGB
                let p = dir.join(format!("{stem}.jpg"));
                dyn_img
                    .to_rgb8()
                    .save_with_format(&p, image::ImageFormat::Jpeg)
                    .map_err(|e| AppError::Internal(format!("保存 JPEG 失败：{e}")))?;
                p.to_string_lossy().into_owned()
            }
            _ => {
                let p = dir.join(format!("{stem}.png"));
                dyn_img
                    .save_with_format(&p, image::ImageFormat::Png)
                    .map_err(|e| AppError::Internal(format!("保存 PNG 失败：{e}")))?;
                p.to_string_lossy().into_owned()
            }
        };
        out.push(path);
        let _ = i; // 保留序号信息可作为调试
    }
    Ok(out)
}

/// 图片 → PDF：把每张图片放在独立页面上。
/// page_size: "fit" = 按图片尺寸生成页面（每页可能不同）；"a4"/"letter" = 统一页面尺寸；"auto" = 全部页面取最大者。
/// layout: "fill" 拉伸铺满；"fit" 按比例居中（默认 fit）。
#[tauri::command]
pub async fn images_to_pdf(
    opts: ImageToPdfOpts,
    output_path: String,
) -> AppResult<String> {
    if opts.image_paths.is_empty() {
        return Err(AppError::NoImagesProvided);
    }
    // 加载所有图片，记录原始尺寸
    let mut imgs: Vec<image::DynamicImage> = Vec::new();
    let mut sizes: Vec<(u32, u32)> = Vec::new();
    for p in &opts.image_paths {
        let im = image::open(p).map_err(|_| AppError::ImageReadFailed { path: p.clone() })?;
        sizes.push((im.width(), im.height()));
        imgs.push(im);
    }

    // 决定每页尺寸
    let page_size_pts: Vec<(f32, f32)> = match opts.page_size.to_lowercase().as_str() {
        "a4" => vec![(595.0, 842.0); imgs.len()],
        "letter" => vec![(612.0, 792.0); imgs.len()],
        "auto" => {
            // 取最大宽、高
            let mw = sizes.iter().map(|(w, _)| *w).max().unwrap_or(595) as f32;
            let mh = sizes.iter().map(|(_, h)| *h).max().unwrap_or(842) as f32;
            vec![(mw, mh); imgs.len()]
        }
        _ => sizes
            .iter()
            .map(|(w, h)| (*w as f32, *h as f32))
            .collect(), // fit
    };

    let _gate = crate::document::pdfium_gate();
    let pdfium = get_pdfium();
    let mut doc = pdfium.create_new_pdf()?;
    {
        let pages = doc.pages_mut();
        for (idx, img) in imgs.iter().enumerate() {
            let (pw, ph) = page_size_pts[idx];
            let mut page = pages.create_page_at_index(
                PdfPagePaperSize::Custom(PdfPoints::new(pw), PdfPoints::new(ph)),
                idx as u16,
            )?;
            let obj_w = img.width() as f32;
            let obj_h = img.height() as f32;
            let (img_w, img_h) = if opts.layout == "fill" {
                // 拉伸铺满
                (pw, ph)
            } else {
                // 保持比例居中
                let sx = pw / obj_w;
                let sy = ph / obj_h;
                let s = sx.min(sy);
                (obj_w * s, obj_h * s)
            };
            let x = (pw - img_w) / 2.0;
            let y = (ph - img_h) / 2.0;
            page.objects_mut().create_image_object(
                PdfPoints::new(x),
                PdfPoints::new(y),
                img,
                Some(PdfPoints::new(img_w.max(1.0))),
                Some(PdfPoints::new(img_h.max(1.0))),
            )?;
        }
    }
    let bytes = doc.save_to_bytes()?;
    std::fs::write(&output_path, &bytes)
        .map_err(|_| AppError::PdfWriteFailed)?;
    Ok(output_path)
}