//! M6 格式转换测试
//!
//! - PDF → PNG / JPEG（按页导出，DPI 可调）
//! - 图片 → PDF（单页 / 多页）
//!
//! 用临时目录做端到端测试，避免依赖 AppState。

use std::fs;
use std::path::PathBuf;

use pdfe_lib::convert::{export_page_to_image_bytes, images_to_pdf_from_images, ImageToPdfOptions};
use pdfium_render::prelude::*;

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    p.push(name);
    p
}

fn pdfium<'a>() -> &'a Pdfium {
    pdfe_lib::pdfium()
}

fn tmp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfe_test_convert_{}", label));
    let _ = fs::create_dir_all(&dir);
    dir
}

// ---------- PDF → 图片 ----------

/// 第 0 页导出为 PNG：文件存在、尺寸正确、非空。
#[test]
fn export_png_first_page() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();

    let png_bytes = export_page_to_image_bytes(&page, 150.0, "png").unwrap();
    assert!(png_bytes.len() > 100, "PNG should have reasonable size");
    // PNG 签名
    assert_eq!(
        &png_bytes[..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    );

    // 验证尺寸：A4 约 595×842 pt，150 DPI = 150/72 ≈ 2.08x
    let img = image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png).unwrap();
    let expected_w = (page.width().value * 150.0 / 72.0).round() as u32;
    let expected_h = (page.height().value * 150.0 / 72.0).round() as u32;
    assert_eq!(img.width(), expected_w);
    assert_eq!(img.height(), expected_h);
}

/// 第 0 页导出为 JPEG：文件存在、非空、格式正确。
#[test]
fn export_jpeg_first_page() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();

    let jpg_bytes = export_page_to_image_bytes(&page, 72.0, "jpeg").unwrap();
    assert!(jpg_bytes.len() > 100, "JPEG should have reasonable size");
    // JPEG 签名 (FF D8 FF)
    assert_eq!(&jpg_bytes[..3], &[0xFF, 0xD8, 0xFF]);

    let img = image::load_from_memory_with_format(&jpg_bytes, image::ImageFormat::Jpeg).unwrap();
    let expected_w = (page.width().value * 72.0 / 72.0).round() as u32;
    let expected_h = (page.height().value * 72.0 / 72.0).round() as u32;
    assert_eq!(img.width(), expected_w);
    assert_eq!(img.height(), expected_h);
}

/// DPI 变化：高 DPI 图更大。
#[test]
fn higher_dpi_produces_larger_image() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();

    let low = export_page_to_image_bytes(&page, 72.0, "png").unwrap();
    let high = export_page_to_image_bytes(&page, 300.0, "png").unwrap();

    let img_low = image::load_from_memory_with_format(&low, image::ImageFormat::Png).unwrap();
    let img_high = image::load_from_memory_with_format(&high, image::ImageFormat::Png).unwrap();

    assert!(img_high.width() > img_low.width());
    assert!(img_high.height() > img_low.height());
    // 300 / 72 ≈ 4.167 倍
    let ratio = img_high.width() as f64 / img_low.width() as f64;
    assert!((ratio - 300.0 / 72.0).abs() < 0.5, "ratio ~= dpi ratio");
}

/// 格式参数大小写不敏感：JPG == jpeg == JPEG。
#[test]
fn format_case_insensitive() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();

    let a = export_page_to_image_bytes(&page, 72.0, "JPEG").unwrap();
    let b = export_page_to_image_bytes(&page, 72.0, "jpg").unwrap();
    assert_eq!(&a[..3], &[0xFF, 0xD8, 0xFF]);
    assert_eq!(&b[..3], &[0xFF, 0xD8, 0xFF]);
}

/// 未知格式回退到 PNG。
#[test]
fn unknown_format_falls_back_to_png() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();

    let result = export_page_to_image_bytes(&page, 72.0, "webp");
    // 未知格式不 panic，应该返回什么？当前代码是走 _ => png 分支
    // 我们验证不返回错误，且输出是 PNG
    assert!(result.is_ok());
    let bytes_out = result.unwrap();
    assert_eq!(
        &bytes_out[..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    );
}

/// DPI 低于下限会被 clamp 到 36。
#[test]
fn dpi_clamped_to_minimum() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();

    // 传 10 DPI（低于 36 下限），结果应该和 36 DPI 一样
    let clamped = export_page_to_image_bytes(&page, 10.0, "png").unwrap();
    let min_dpi = export_page_to_image_bytes(&page, 36.0, "png").unwrap();

    let img_clamped =
        image::load_from_memory_with_format(&clamped, image::ImageFormat::Png).unwrap();
    let img_min = image::load_from_memory_with_format(&min_dpi, image::ImageFormat::Png).unwrap();

    assert_eq!(img_clamped.width(), img_min.width());
    assert_eq!(img_clamped.height(), img_min.height());
}

/// DPI 高于上限会被 clamp 到 600。
#[test]
fn dpi_clamped_to_maximum() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();

    let clamped = export_page_to_image_bytes(&page, 2000.0, "png").unwrap();
    let max_dpi = export_page_to_image_bytes(&page, 600.0, "png").unwrap();

    let img_clamped =
        image::load_from_memory_with_format(&clamped, image::ImageFormat::Png).unwrap();
    let img_max = image::load_from_memory_with_format(&max_dpi, image::ImageFormat::Png).unwrap();

    assert_eq!(img_clamped.width(), img_max.width());
    assert_eq!(img_clamped.height(), img_max.height());
}

/// 导出多页：每页都生成独立文件。
#[test]
fn export_multiple_pages() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let total = doc.pages().len();
    if total < 2 {
        println!("sample.pdf 只有 1 页，跳过多页测试");
        return;
    }
    let pages = doc.pages();

    let dir = tmp_dir("multi_page");
    let mut paths = Vec::new();

    for i in 0..total.min(3) {
        let page = pages.get(i).unwrap();
        let png = export_page_to_image_bytes(&page, 72.0, "png").unwrap();
        let p = dir.join(format!("page_{:04}.png", i + 1));
        fs::write(&p, &png).unwrap();
        paths.push(p);
    }

    for p in &paths {
        assert!(p.exists(), "{} should exist", p.display());
        assert!(p.metadata().unwrap().len() > 0);
    }

    // 清理
    for p in &paths {
        let _ = fs::remove_file(p);
    }
    let _ = fs::remove_dir(&dir);
}

// ---------- 图片 → PDF ----------

/// 单张图片生成 PDF：输出是有效 PDF，有 1 页。
#[test]
fn single_image_to_pdf() {
    // 生成一张测试图片
    let img = image::RgbaImage::from_fn(200, 150, |x, y| image::Rgba([x as u8, y as u8, 128, 255]));
    let dyn_img = image::DynamicImage::ImageRgba8(img);

    let opts = ImageToPdfOptions {
        page_size: "fit".into(),
        layout: "fit".into(),
    };
    let bytes = images_to_pdf_from_images(&[dyn_img], &opts).unwrap();

    assert!(bytes.len() > 20);
    // PDF 签名
    assert_eq!(&bytes[..5], b"%PDF-");

    // 验证能被 pdfium 打开
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    assert_eq!(doc.pages().len(), 1);
}

/// 多张图片生成 PDF：页数匹配。
#[test]
fn multiple_images_to_pdf() {
    let mut imgs = Vec::new();
    for i in 0..3 {
        let img = image::RgbaImage::from_fn(100 + i * 20, 80 + i * 10, |x, y| {
            image::Rgba([(x + i * 50) as u8, y as u8, 200, 255])
        });
        imgs.push(image::DynamicImage::ImageRgba8(img));
    }

    let opts = ImageToPdfOptions {
        page_size: "fit".into(),
        layout: "fit".into(),
    };
    let bytes = images_to_pdf_from_images(&imgs, &opts).unwrap();

    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    assert_eq!(doc.pages().len(), 3);
}

/// page_size = a4：所有页面都是 A4 尺寸。
#[test]
fn image_to_pdf_a4_size() {
    let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        100,
        100,
        image::Rgba([255, 0, 0, 255]),
    ));
    let opts = ImageToPdfOptions {
        page_size: "a4".into(),
        layout: "fit".into(),
    };
    let bytes = images_to_pdf_from_images(&[img], &opts).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    assert!((page.width().value - 595.0).abs() < 1.0, "width ~= 595 pt");
    assert!(
        (page.height().value - 842.0).abs() < 1.0,
        "height ~= 842 pt"
    );
}

/// layout = fill：图片拉伸铺满页面。
#[test]
fn image_to_pdf_fill_layout() {
    let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        200,
        100,
        image::Rgba([0, 255, 0, 255]),
    ));
    let opts = ImageToPdfOptions {
        page_size: "fit".into(),
        layout: "fill".into(),
    };
    let bytes = images_to_pdf_from_images(&[img], &opts).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    // fit + fill 模式下页面尺寸 = 图片尺寸
    assert!((page.width().value - 200.0).abs() < 1.0);
    assert!((page.height().value - 100.0).abs() < 1.0);
}

/// 空图片列表返回错误。
#[test]
fn empty_images_returns_error() {
    let opts = ImageToPdfOptions {
        page_size: "fit".into(),
        layout: "fit".into(),
    };
    let result = images_to_pdf_from_images(&[], &opts);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "no_images_provided");
}

// ---------- 往返测试：PDF → 图片 → PDF ----------

/// PDF 第一页导出为图片，再把图片生成新 PDF，新 PDF 应该有相同数量的页。
#[test]
fn pdf_to_image_to_pdf_roundtrip() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();

    let png = export_page_to_image_bytes(&page, 150.0, "png").unwrap();
    let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap();

    let opts = ImageToPdfOptions {
        page_size: "fit".into(),
        layout: "fit".into(),
    };
    let new_pdf = images_to_pdf_from_images(&[img], &opts).unwrap();

    let new_doc = pdfium().load_pdf_from_byte_slice(&new_pdf, None).unwrap();
    assert_eq!(new_doc.pages().len(), 1);

    // 页面尺寸应该匹配图片尺寸（像素数直接当 pt）
    let new_page = new_doc.pages().get(0).unwrap();
    let new_w = new_page.width().value;
    let new_h = new_page.height().value;
    let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap();
    assert!((new_w - img.width() as f32).abs() < 1.0);
    assert!((new_h - img.height() as f32).abs() < 1.0);
}
