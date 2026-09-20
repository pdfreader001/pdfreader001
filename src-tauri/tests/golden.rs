use std::fs;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// 打开样例 PDF → 校验页数与页面尺寸。
#[test]
fn open_sample_document() {
    let bytes = fs::read(fixture("sample.pdf")).expect("fixture 缺失");
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium
        .load_pdf_from_byte_slice(&bytes, None)
        .expect("解析样例 PDF 失败");
    assert_eq!(doc.pages().len(), 2);

    let page = doc.pages().get(0).unwrap();
    assert!((page.width().value - 612.0).abs() < 0.01);
    assert!((page.height().value - 792.0).abs() < 0.01);
}

/// 渲染第 1 页 → 校验位图尺寸。
#[test]
fn render_first_page() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    let bitmap = page.render(612, 792, None).unwrap();
    assert_eq!(bitmap.width(), 612);
    assert_eq!(bitmap.height(), 792);
    let rgba = bitmap.as_rgba_bytes();
    assert_eq!(rgba.len(), 612 * 792 * 4);
}

/// 保存 → 重新打开往返。
#[test]
fn save_roundtrip() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let saved = doc.save_to_bytes().unwrap();

    let doc2 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    assert_eq!(doc2.pages().len(), 2);
}

/// 提取页面文本。
#[test]
fn extract_text() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    let text = page.text().unwrap().all();
    assert!(text.contains("Hello PDFe Page 1"), "实际文本: {text:?}");
}

/// 损坏字节流应报错。
#[test]
fn damaged_document() {
    let bytes = b"not a pdf at all".to_vec();
    let pdfium = pdfe_lib::pdfium();
    assert!(pdfium.load_pdf_from_byte_slice(&bytes, None).is_err());
}

/// 旋转页面 → 校验旋转状态可保存并读取。
#[test]
fn rotate_page_persists() {
    use pdfium_render::prelude::PdfPageRenderRotation;
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    {
        let mut page = doc.pages().get(0).unwrap();
        page.set_rotation(PdfPageRenderRotation::Degrees90);
    }
    let saved = doc.save_to_bytes().unwrap();
    let doc2 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    let page = doc2.pages().get(0).unwrap();
    let rot = page.rotation().unwrap();
    assert!(
        matches!(rot, PdfPageRenderRotation::Degrees90),
        "期望旋转 90°，实际: {:?}",
        rot
    );
}

/// 删除页面 → 校验页数减少。
#[test]
fn delete_page_reduces_count() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    assert_eq!(doc.pages().len(), 2);
    let page = doc.pages().get(1).unwrap();
    page.delete().unwrap();
    assert_eq!(doc.pages().len(), 1);
}

/// 复制页面 → 校验页数增加。
#[test]
fn copy_page_increases_count() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let src_doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let mut doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    assert_eq!(doc.pages().len(), 2);
    doc.pages_mut().copy_page_from_document(&src_doc, 0, 2).unwrap();
    assert_eq!(doc.pages().len(), 3);
}

/// 插入空白页 → 校验尺寸与页数。
#[test]
fn insert_blank_page() {
    use pdfium_render::prelude::PdfPagePaperSize;
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    assert_eq!(doc.pages().len(), 3);
    let page = doc.pages().get(0).unwrap();
    assert!((page.width().value - 595.0).abs() < 1.0);
    assert!((page.height().value - 842.0).abs() < 1.0);
}

/// 跨文档复制 → 合并两页。
#[test]
fn copy_between_documents() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let mut new_doc = pdfium.create_new_pdf().unwrap();
    let size = pdfium_render::prelude::PdfPagePaperSize::a4();
    new_doc.pages_mut().create_page_at_index(size, 0).unwrap();
    new_doc
        .pages_mut()
        .copy_page_range_from_document(&doc, 0..=1, 1)
        .unwrap();
    assert_eq!(new_doc.pages().len(), 3);
}

/// 新文档保存往返 → 页数一致。
#[test]
fn new_document_save_roundtrip() {
    use pdfium_render::prelude::PdfPagePaperSize;
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    doc.pages_mut().create_page_at_index(size, 1).unwrap();
    let saved = doc.save_to_bytes().unwrap();
    let doc2 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    assert_eq!(doc2.pages().len(), 2);
}

/// 文字水印 → 保存往返后文本可提取（含 alpha 填充色与旋转）。
#[test]
fn text_watermark_extractable() {
    use pdfium_render::prelude::{
        PdfColor, PdfPageObjectCommon, PdfPageObjectsCommon, PdfPagePaperSize, PdfPoints,
    };
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    // 先取字体 token，避免与 pages_mut 借用冲突
    let token = doc.fonts_mut().helvetica();
    {
        let mut pages = doc.pages_mut();
        let mut page = pages.get(0).unwrap();
        let mut obj = page
            .objects_mut()
            .create_text_object(
                PdfPoints::new(100.0),
                PdfPoints::new(400.0),
                "CONFIDENTIAL",
                token,
                PdfPoints::new(48.0),
            )
            .expect("创建文本对象失败");
        obj.set_fill_color(PdfColor::new(255, 0, 0, 100)).unwrap();
        obj.rotate_clockwise_degrees(45.0).unwrap();
    }
    let saved = doc.save_to_bytes().unwrap();
    let doc2 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    let text = doc2.pages().get(0).unwrap().text().unwrap().all();
    assert!(text.contains("CONFIDENTIAL"), "水印文本应可提取: {text:?}");
}

/// 图片水印 → 对象数量增加且可保存往返。
#[test]
fn image_watermark_persists() {
    use pdfium_render::prelude::{PdfPageObjectsCommon, PdfPagePaperSize, PdfPoints};
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    // 生成 8x8 红色纯色图作为水印
    let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        8,
        8,
        image::Rgba([255, 0, 0, 128]),
    ));
    {
        let mut pages = doc.pages_mut();
        let mut page = pages.get(0).unwrap();
        page.objects_mut()
            .create_image_object(
                PdfPoints::new(100.0),
                PdfPoints::new(400.0),
                &img,
                Some(PdfPoints::new(100.0)),
                Some(PdfPoints::new(100.0)),
            )
            .expect("创建图片对象失败");
    }
    let saved = doc.save_to_bytes().unwrap();
    let doc2 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    let page = doc2.pages().get(0).unwrap();
    assert_eq!(page.objects().len(), 1, "应包含 1 个水印图片对象");
}
