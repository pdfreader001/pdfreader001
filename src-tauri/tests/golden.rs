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
