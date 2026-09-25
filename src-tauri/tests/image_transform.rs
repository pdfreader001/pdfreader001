//! 图片对象移动/缩放（M5 深度编辑补强）集成测试。
//!
//! 这些测试直接调用命令背后的 pub 纯函数（`list_image_objects_logic` /
//! `set_image_bounds_logic`），在真实 pdfium 运行路径下验证：
//! - 能列出页面上的图片对象及其包围盒
//! - `set_image_bounds` 能把图片包围盒精确映射到目标矩形（移动 + 缩放）
//! - 非图片对象、非法矩形、越界索引都返回稳定的错误码

use pdfe_lib::edit_ext::{list_image_objects_logic, set_image_bounds_logic, PtRect};
use pdfium_render::prelude::*;

fn pdfium<'a>() -> &'a Pdfium {
    pdfe_lib::pdfium()
}

/// 生成一个带单张红色图片的单页 PDF。
/// 图片放在 (100, 400)，尺寸 100x80 PDF 点。
fn make_image_pdf() -> Vec<u8> {
    let pdfium = pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        40,
        30,
        image::Rgba([200, 30, 30, 255]),
    ));
    {
        let pages = doc.pages_mut();
        let mut page = pages.get(0).unwrap();
        page.objects_mut()
            .create_image_object(
                PdfPoints::new(100.0),
                PdfPoints::new(400.0),
                &img,
                Some(PdfPoints::new(100.0)),
                Some(PdfPoints::new(80.0)),
            )
            .unwrap();
    }
    doc.save_to_bytes().unwrap()
}

/// 生成一个只有文字对象的单页 PDF（用于验证「非图片对象」错误分支）。
fn make_text_pdf() -> Vec<u8> {
    let pdfium = pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    let token = doc.fonts_mut().helvetica();
    {
        let pages = doc.pages_mut();
        let mut page = pages.get(0).unwrap();
        page.objects_mut()
            .create_text_object(
                PdfPoints::new(72.0),
                PdfPoints::new(700.0),
                "not an image",
                token,
                PdfPoints::new(24.0),
            )
            .unwrap();
    }
    doc.save_to_bytes().unwrap()
}

/// 读取页面第一个图片对象的包围盒（left, bottom, right, top）。
fn first_image_bounds(bytes: &[u8]) -> (f32, f32, f32, f32) {
    let list = list_image_objects_logic(pdfium(), bytes, 0).unwrap();
    let first = list.first().expect("expected at least one image object");
    (first.left, first.bottom, first.right, first.top)
}

fn assert_close(actual: f32, expected: f32, label: &str) {
    assert!(
        (actual - expected).abs() < 1.0,
        "{label}: expected ~{expected}, got {actual}"
    );
}

// ---------- list ----------

#[test]
fn list_returns_the_single_image() {
    let bytes = make_image_pdf();
    let list = list_image_objects_logic(pdfium(), &bytes, 0).unwrap();
    assert_eq!(list.len(), 1, "expected exactly one image object");
    assert_eq!(list[0].object_index, 0);

    assert_close(list[0].left, 100.0, "left");
    assert_close(list[0].bottom, 400.0, "bottom");
    assert_close(list[0].right, 200.0, "right");
    assert_close(list[0].top, 480.0, "top");
}

#[test]
fn list_ignores_text_objects() {
    let bytes = make_text_pdf();
    let list = list_image_objects_logic(pdfium(), &bytes, 0).unwrap();
    assert!(
        list.is_empty(),
        "text-only page should expose no image objects"
    );
}

// ---------- set bounds ----------

#[test]
fn set_bounds_moves_and_scales_the_image() {
    let bytes = make_image_pdf();
    let target = PtRect {
        left: 150.0,
        bottom: 250.0,
        right: 350.0,
        top: 450.0,
    };

    let out = set_image_bounds_logic(pdfium(), &bytes, 0, 0, target).unwrap();
    let (l, b, r, t) = first_image_bounds(&out);

    assert_close(l, target.left, "left");
    assert_close(b, target.bottom, "bottom");
    assert_close(r, target.right, "right");
    assert_close(t, target.top, "top");
}

#[test]
fn set_bounds_pure_translation_keeps_size() {
    let bytes = make_image_pdf();
    // 平移 (25, -100)，尺寸不变。
    let target = PtRect {
        left: 125.0,
        bottom: 300.0,
        right: 225.0,
        top: 380.0,
    };

    let out = set_image_bounds_logic(pdfium(), &bytes, 0, 0, target).unwrap();
    let (l, b, r, t) = first_image_bounds(&out);

    assert_close(l, 125.0, "left");
    assert_close(b, 300.0, "bottom");
    assert_close(r - l, 100.0, "width");
    assert_close(t - b, 80.0, "height");
}

// ---------- error paths ----------

#[test]
fn set_bounds_rejects_non_image_object() {
    let bytes = make_text_pdf();
    let target = PtRect {
        left: 0.0,
        bottom: 0.0,
        right: 100.0,
        top: 100.0,
    };
    let err = set_image_bounds_logic(pdfium(), &bytes, 0, 0, target).unwrap_err();
    assert_eq!(err.code(), "object_not_image");
}

#[test]
fn set_bounds_rejects_invalid_rect() {
    let bytes = make_image_pdf();
    let target = PtRect {
        left: 100.0,
        bottom: 100.0,
        right: 100.0,
        top: 200.0,
    };
    let err = set_image_bounds_logic(pdfium(), &bytes, 0, 0, target).unwrap_err();
    assert_eq!(err.code(), "invalid_rect");
}

#[test]
fn set_bounds_rejects_out_of_range_index() {
    let bytes = make_image_pdf();
    let target = PtRect {
        left: 0.0,
        bottom: 0.0,
        right: 100.0,
        top: 100.0,
    };
    let err = set_image_bounds_logic(pdfium(), &bytes, 0, 99, target).unwrap_err();
    assert_eq!(err.code(), "page_out_of_range");
}

#[test]
fn set_bounds_rejects_out_of_range_page() {
    let bytes = make_image_pdf();
    let target = PtRect {
        left: 0.0,
        bottom: 0.0,
        right: 100.0,
        top: 100.0,
    };
    let err = set_image_bounds_logic(pdfium(), &bytes, 5, 0, target).unwrap_err();
    assert_eq!(err.code(), "page_out_of_range");
}
