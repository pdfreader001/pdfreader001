//! Deep edit (M5) tests.
//!
//! Exercises the public command logic end-to-end without Tauri state: we
//! create a small PDF, then directly call the same code path the commands
//! execute to assert that:
//! - rewriting text produces a document with the new string in its text layer
//! - adding a text box increases the page's text count
//! - scanning detection returns true for an image-only page and false for a
//!   page with text content

use pdfium_render::prelude::*;
use pdfe_lib::edit_ext::{
    add_text_box_logic, is_scanned_page_logic, AddTextBoxOpts, PtRect, RewriteTextOpts,
    rewrite_text_logic,
};

fn pdfium<'a>() -> &'a Pdfium {
    pdfe_lib::pdfium()
}

/// 创建一个带文字的单页测试 PDF，返回 bytes。
fn make_text_pdf(text: &str) -> Vec<u8> {
    let pdfium = pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    let token = doc.fonts_mut().helvetica();
    {
        let pages = doc.pages_mut();
        let mut page = pages.get(0).unwrap();
        let objects = page.objects_mut();
        let _ = objects
            .create_text_object(
                PdfPoints::new(72.0),
                PdfPoints::new(700.0),
                text,
                token,
                PdfPoints::new(24.0),
            )
            .unwrap();
    }
    doc.save_to_bytes().unwrap()
}

// ---------- PtRect ----------

#[test]
fn pt_rect_validation() {
    let valid = PtRect { left: 0.0, bottom: 0.0, right: 100.0, top: 100.0 };
    assert!(valid.right > valid.left && valid.top > valid.bottom);
    let zero_w = PtRect { left: 50.0, bottom: 0.0, right: 50.0, top: 100.0 };
    assert!(!(zero_w.right > zero_w.left));
}

// ---------- scan detection ----------

/// A page with substantial text is NOT scanned.
#[test]
fn text_page_is_not_scanned() {
    let bytes = make_text_pdf("Hello scan detection");
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let scanned = is_scanned_page_logic(&doc, 0);
    assert!(!scanned, "page with text should not be detected as scanned");
}

/// A blank (or image-only) page IS detected as scanned.
#[test]
fn blank_page_is_scanned() {
    let pdfium = pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    let bytes = doc.save_to_bytes().unwrap();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let scanned = is_scanned_page_logic(&doc, 0);
    assert!(scanned, "blank page should be detected as scanned");
}

// ---------- rewrite_text ----------

/// 文字重写：仅插入新文字（不依赖 pdfium 删除路径）。验证返回 bytes 有效。
/// 注：删除 PDF 内置文本对象在某些 pdfium 版本下会触发 FFI 访问违例，
/// 因此本测试只验证"插入"路径的安全性与正确性，"删除 + 覆盖"路径留待
/// 真实 PDF（带更复杂内容流）由 E2E 验证。
#[test]
fn rewrite_text_insert_only_produces_valid_pdf() {
    let pdfium = pdfium();
    let bytes = make_text_pdf("Baseline");

    let region = PtRect {
        left: 500.0,
        bottom: 50.0,
        right: 590.0,
        top: 100.0,
    };
    let opts = RewriteTextOpts {
        new_text: "Inserted".into(),
        font_size: 18.0,
        color: "#FF0000".into(),
        font_name: None,
    };
    let (new_bytes, _approximated) = rewrite_text_logic(pdfium, &bytes, 0, region, &opts).unwrap();
    assert!(!new_bytes.is_empty());
    // 输出能被 pdfium 重新打开
    let _doc = pdfium.load_pdf_from_byte_slice(&new_bytes, None).unwrap();
}

/// 文字重写：空文本返回错误。
#[test]
fn rewrite_text_empty_string_errors() {
    let pdfium = pdfium();
    let bytes = make_text_pdf("Hello");
    let region = PtRect { left: 0.0, bottom: 0.0, right: 100.0, top: 100.0 };
    let opts = RewriteTextOpts {
        new_text: "".into(),
        font_size: 12.0,
        color: "#000000".into(),
        font_name: None,
    };
    let result = rewrite_text_logic(pdfium, &bytes, 0, region, &opts);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "text_empty");
}

/// 文字重写：无效矩形（零宽度）返回错误。
#[test]
fn rewrite_text_invalid_rect_errors() {
    let pdfium = pdfium();
    let bytes = make_text_pdf("Hello");
    // right <= left  →  invalid
    let region = PtRect { left: 100.0, bottom: 0.0, right: 50.0, top: 100.0 };
    let opts = RewriteTextOpts {
        new_text: "test".into(),
        font_size: 12.0,
        color: "#000000".into(),
        font_name: None,
    };
    let result = rewrite_text_logic(pdfium, &bytes, 0, region, &opts);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "invalid_rect");
}

/// 文字重写：页码越界返回错误。
#[test]
fn rewrite_text_page_out_of_range() {
    let pdfium = pdfium();
    let bytes = make_text_pdf("Hello");
    let region = PtRect { left: 0.0, bottom: 0.0, right: 100.0, top: 100.0 };
    let opts = RewriteTextOpts {
        new_text: "test".into(),
        font_size: 12.0,
        color: "#000000".into(),
        font_name: None,
    };
    let result = rewrite_text_logic(pdfium, &bytes, 99, region, &opts);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "page_out_of_range");
}

/// 文字重写：region 完全不包含任何文字 → 仍然成功（只插入新文字，不删除）。
#[test]
fn rewrite_text_outside_region_still_inserts() {
    let pdfium = pdfium();
    let bytes = make_text_pdf("KeepMe");

    // region 放在远离文字的右下角
    let region = PtRect {
        left: 500.0,
        bottom: 50.0,
        right: 590.0,
        top: 100.0,
    };
    let opts = RewriteTextOpts {
        new_text: "NewText".into(),
        font_size: 12.0,
        color: "#000000".into(),
        font_name: None,
    };
    let (new_bytes, _approximated) = rewrite_text_logic(pdfium, &bytes, 0, region, &opts).unwrap();
    let doc_after = pdfium.load_pdf_from_byte_slice(&new_bytes, None).unwrap();
    let text_after = doc_after.pages().get(0).unwrap().text().unwrap().all();
    // 原文保留（因为 region 没命中）+ 新文字插入
    assert!(text_after.contains("KeepMe"), "original text outside region should remain");
    assert!(text_after.contains("NewText"), "new text should be inserted at region");
}

/// 文字重写：原字体名无法映射到系统字体 → 标记为「近似替换」。
#[test]
fn rewrite_text_with_unknown_font_marks_approximated() {
    let pdfium = pdfium();
    let bytes = make_text_pdf("Baseline");

    let region = PtRect {
        left: 500.0,
        bottom: 50.0,
        right: 590.0,
        top: 100.0,
    };
    let opts = RewriteTextOpts {
        new_text: "Inserted".into(),
        font_size: 18.0,
        color: "#000000".into(),
        font_name: Some("NoSuchFont-1234".into()),
    };
    let (new_bytes, approximated) = rewrite_text_logic(pdfium, &bytes, 0, region, &opts).unwrap();
    assert!(!new_bytes.is_empty());
    assert!(approximated, "unknown source font should be flagged as approximated");
}

// ---------- add_text_box ----------

/// 新增文本框：文字出现在文本层中。
#[test]
fn add_text_box_inserts_text() {
    let pdfium = pdfium();
    let bytes = make_text_pdf("Original");

    let opts = AddTextBoxOpts {
        text: "AddedText".into(),
        font_size: 18.0,
        color: "#FF0000".into(),
        x: 100.0,
        y: 600.0,
    };
    let new_bytes = add_text_box_logic(pdfium, &bytes, 0, &opts).unwrap();
    let doc_after = pdfium.load_pdf_from_byte_slice(&new_bytes, None).unwrap();
    let text_after = doc_after.pages().get(0).unwrap().text().unwrap().all();
    assert!(text_after.contains("Original"), "original text should remain");
    assert!(text_after.contains("AddedText"), "new text should appear");
}

/// 新增文本框：空文本返回错误。
#[test]
fn add_text_box_empty_errors() {
    let pdfium = pdfium();
    let bytes = make_text_pdf("Hello");
    let opts = AddTextBoxOpts {
        text: "".into(),
        font_size: 12.0,
        color: "#000000".into(),
        x: 100.0,
        y: 100.0,
    };
    let result = add_text_box_logic(pdfium, &bytes, 0, &opts);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "text_empty");
}

/// 新增文本框：页码越界返回错误。
#[test]
fn add_text_box_page_out_of_range() {
    let pdfium = pdfium();
    let bytes = make_text_pdf("Hello");
    let opts = AddTextBoxOpts {
        text: "test".into(),
        font_size: 12.0,
        color: "#000000".into(),
        x: 100.0,
        y: 100.0,
    };
    let result = add_text_box_logic(pdfium, &bytes, 99, &opts);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "page_out_of_range");
}

/// 新增文本框：损坏 PDF 返回错误。
#[test]
fn add_text_box_corrupted_pdf_errors() {
    let pdfium = pdfium();
    let bytes = b"not a pdf";
    let opts = AddTextBoxOpts {
        text: "test".into(),
        font_size: 12.0,
        color: "#000000".into(),
        x: 100.0,
        y: 100.0,
    };
    let result = add_text_box_logic(pdfium, bytes, 0, &opts);
    assert!(result.is_err());
}

/// 新增文本框：多页 PDF，在第 2 页添加文字。
#[test]
fn add_text_box_second_page() {
    let pdfium = pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    doc.pages_mut().create_page_at_index(size, 1).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let opts = AddTextBoxOpts {
        text: "PageTwoText".into(),
        font_size: 16.0,
        color: "#0000FF".into(),
        x: 72.0,
        y: 700.0,
    };
    let new_bytes = add_text_box_logic(pdfium, &bytes, 1, &opts).unwrap();
    let doc_after = pdfium.load_pdf_from_byte_slice(&new_bytes, None).unwrap();
    assert_eq!(doc_after.pages().len(), 2);
    // 第 1 页应该没有新文字
    let p0_text = doc_after.pages().get(0).unwrap().text().unwrap().all();
    assert!(!p0_text.contains("PageTwoText"));
    // 第 2 页应该有
    let p1_text = doc_after.pages().get(1).unwrap().text().unwrap().all();
    assert!(p1_text.contains("PageTwoText"));
}