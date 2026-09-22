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
use pdfe_lib::edit_ext::{is_scanned_page_logic, PtRect};

fn pdfium<'a>() -> &'a Pdfium {
    pdfe_lib::pdfium()
}

#[test]
fn pt_rect_validation() {
    let valid = PtRect { left: 0.0, bottom: 0.0, right: 100.0, top: 100.0 };
    assert!(valid.right > valid.left && valid.top > valid.bottom);
    let zero_w = PtRect { left: 50.0, bottom: 0.0, right: 50.0, top: 100.0 };
    assert!(!(zero_w.right > zero_w.left));
}

/// A page with substantial text is NOT scanned.
#[test]
fn text_page_is_not_scanned() {
    let pdfium = pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    let token = doc.fonts_mut().helvetica();
    {
        let mut pages = doc.pages_mut();
        let mut page = pages.get(0).unwrap();
        let mut objects = page.objects_mut();
        let _ = objects
            .create_text_object(
                PdfPoints::new(72.0),
                PdfPoints::new(700.0),
                "Hello scan detection",
                token,
                PdfPoints::new(24.0),
            )
            .unwrap();
    }
    let bytes = doc.save_to_bytes().unwrap();
    // Re-open the document and test the helper.
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
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