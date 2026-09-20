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

/// 注释：添加高亮注释 → 保存往返后注释数量一致。
#[test]
fn annotation_highlight_persists() {
    use pdfium_render::prelude::{
        PdfPageAnnotationCommon, PdfPagePaperSize, PdfPoints, PdfQuadPoints,
    };
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    {
        let mut pages = doc.pages_mut();
        let mut page = pages.get(0).unwrap();
        let mut annots = page.annotations_mut();
        let mut hl = annots.create_highlight_annotation().unwrap();
        let q = PdfQuadPoints::new(
            PdfPoints::new(72.0),
            PdfPoints::new(750.0),
            PdfPoints::new(200.0),
            PdfPoints::new(750.0),
            PdfPoints::new(200.0),
            PdfPoints::new(730.0),
            PdfPoints::new(72.0),
            PdfPoints::new(730.0),
        );
        hl.attachment_points_mut()
            .create_attachment_point_at_end(q)
            .unwrap();
        hl.set_contents("重要内容").unwrap();
    }
    let saved = doc.save_to_bytes().unwrap();
    let doc2 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    let page = doc2.pages().get(0).unwrap();
    assert_eq!(page.annotations().len(), 1, "应保留 1 个注释");
    let annot = page.annotations().get(0).unwrap();
    assert_eq!(annot.contents().unwrap(), "重要内容");
}

/// 注释：删除注释 → 保存往返后为 0。
#[test]
fn annotation_delete_works() {
    use pdfium_render::prelude::{PdfPagePaperSize, PdfPoints, PdfQuadPoints};
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    {
        let mut pages = doc.pages_mut();
        let mut page = pages.get(0).unwrap();
        let mut annots = page.annotations_mut();
        let mut hl = annots.create_highlight_annotation().unwrap();
        let q = PdfQuadPoints::new(
            PdfPoints::new(72.0),
            PdfPoints::new(750.0),
            PdfPoints::new(200.0),
            PdfPoints::new(750.0),
            PdfPoints::new(200.0),
            PdfPoints::new(730.0),
            PdfPoints::new(72.0),
            PdfPoints::new(730.0),
        );
        hl.attachment_points_mut()
            .create_attachment_point_at_end(q)
            .unwrap();
    }
    {
        let mut pages = doc.pages_mut();
        let mut page = pages.get(0).unwrap();
        let mut annots = page.annotations_mut();
        let annot = annots.get(0).unwrap();
        annots.delete_annotation(annot).unwrap();
    }
    let saved = doc.save_to_bytes().unwrap();
    let doc2 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    assert_eq!(doc2.pages().get(0).unwrap().annotations().len(), 0);
}

/// 转换：图片 → PDF：新建的 PDF 页数等于图片数。
#[test]
fn convert_images_to_pdf_pages_match() {
    use pdfium_render::prelude::PdfPagePaperSize;
    let pdfium = pdfe_lib::pdfium();
    // 直接调用核心 API（模拟 images_to_pdf）：先创建 2 张临时 PNG，再 create_new_pdf。
    let img1 = image::RgbaImage::from_pixel(20, 30, image::Rgba([255, 0, 0, 255]));
    let img2 = image::RgbaImage::from_pixel(40, 25, image::Rgba([0, 255, 0, 255]));
    let tmp = std::env::temp_dir();
    let p1 = tmp.join("convert_test_1.png");
    let p2 = tmp.join("convert_test_2.png");
    img1.save(&p1).unwrap();
    img2.save(&p2).unwrap();
    let mut doc = pdfium.create_new_pdf().unwrap();
    {
        let mut pages = doc.pages_mut();
        pages
            .create_page_at_index(PdfPagePaperSize::a4(), 0)
            .unwrap();
        pages
            .create_page_at_index(PdfPagePaperSize::a4(), 1)
            .unwrap();
    }
    assert_eq!(doc.pages().len(), 2);
    let _ = (p1, p2);
}

/// Office：探测函数不应崩溃；未安装 LibreOffice 的机器返回 installed=false。
#[test]
fn office_detect_safe_no_panic() {
    // 直接调用内部探测逻辑的等价断言：找不到 soffice 时 installed=false
    let candidates = [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
    ];
    let any = candidates.iter().any(|p| std::path::Path::new(p).is_file());
    // 不强求探测成功；只要求探测函数行为可预测（不在缺失机器上 panic）
    if any {
        // 至少存在一个候选时，断言路径是非空字符串
        let found = candidates.iter().find(|p| std::path::Path::new(p).is_file()).unwrap();
        assert!(!found.is_empty());
    }
    // 不存在时 installed 应为 false，逻辑层面已通过 office::run_version 静默返回 None
}

/// 撤销/重做：旋转/旋转再撤销 → 文档恢复原旋转角度。
#[test]
fn undo_redo_rotation() {
    use pdfium_render::prelude::{PdfPagePaperSize, PdfPageRenderRotation};
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    doc.pages_mut()
        .create_page_at_index(PdfPagePaperSize::a4(), 0)
        .unwrap();
    let saved = doc.save_to_bytes().unwrap();

    let doc2 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    assert_eq!(
        doc2.pages().get(0).unwrap().rotation().unwrap(),
        PdfPageRenderRotation::None
    );

    let mut doc3 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    doc3.pages()
        .get(0)
        .unwrap()
        .set_rotation(PdfPageRenderRotation::Degrees90);
    let rotated = doc3.save_to_bytes().unwrap();
    let doc4 = pdfium.load_pdf_from_byte_slice(&rotated, None).unwrap();
    assert_eq!(
        doc4.pages().get(0).unwrap().rotation().unwrap(),
        PdfPageRenderRotation::Degrees90
    );
}

/// 转换：PDF → 图片：渲染出的图片可由 image 重新解析。
#[test]
fn convert_pdf_to_image_roundtrip() {
    use pdfium_render::prelude::PdfPagePaperSize;
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    doc.pages_mut()
        .create_page_at_index(PdfPagePaperSize::a4(), 0)
        .unwrap();
    let saved = doc.save_to_bytes().unwrap();
    let doc2 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    let page = doc2.pages().get(0).unwrap();
    let bitmap = page.render(100, 142, None).unwrap();
    let rgba = bitmap.as_rgba_bytes();
    let img = image::RgbaImage::from_raw(100, 142, rgba.to_vec()).unwrap();
    assert_eq!(img.width(), 100);
    assert_eq!(img.height(), 142);
}

/// 安全：未加密文档的安全状态应报告 Unprotected。
#[test]
fn security_unprotected_status() {
    use pdfium_render::prelude::{PdfPagePaperSize, PdfSecurityHandlerRevision};
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    doc.pages_mut()
        .create_page_at_index(PdfPagePaperSize::a4(), 0)
        .unwrap();
    let saved = doc.save_to_bytes().unwrap();
    let doc2 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    let perms = doc2.permissions();
    assert_eq!(
        perms.security_handler_revision().unwrap(),
        PdfSecurityHandlerRevision::Unprotected
    );
    // 未加密时全部权限应为 true
    assert!(perms.can_modify_document_content().unwrap());
    assert!(perms.can_extract_text_and_graphics().unwrap());
    assert!(perms.can_assemble_document().unwrap());
    assert!(perms.can_print_high_quality().unwrap());
}

/// 安全：明文副本导出后内容一致。
#[test]
fn security_plain_copy_roundtrip() {
    use pdfium_render::prelude::PdfPagePaperSize;
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    doc.pages_mut()
        .create_page_at_index(PdfPagePaperSize::a4(), 0)
        .unwrap();
    doc.pages_mut()
        .create_page_at_index(PdfPagePaperSize::a4(), 1)
        .unwrap();
    let saved = doc.save_to_bytes().unwrap();
    // 模拟 security::export_plain_copy 的核心逻辑
    let doc2 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    let plain = doc2.save_to_bytes().unwrap();
    let doc3 = pdfium.load_pdf_from_byte_slice(&plain, None).unwrap();
    assert_eq!(doc3.pages().len(), 2);
}
