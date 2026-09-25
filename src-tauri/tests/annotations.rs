//! M5 注释（annotation）测试：list / add（6 种类型）/ delete
//!
//! 用法：在 Cargo workspace 用 `cargo test --test annotations` 即可
//! 全部走纯函数路径（不依赖 Tauri State）。

use pdfium_render::prelude::*;
use pdfe_lib::edit::{
    add_annotation_logic, delete_annotation_logic, list_annotations_logic, AddAnnotationOpts,
    AnnotationKind, RegionSpec,
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
                PdfPoints::new(720.0),
                text,
                token,
                PdfPoints::new(18.0),
            )
            .unwrap();
    }
    doc.save_to_bytes().unwrap()
}

/// 标准区域（页面中部归一化 0.1~0.3 x 0.1~0.2）。
fn region() -> RegionSpec {
    RegionSpec {
        left: 0.1,
        top: 0.1,
        width: 0.2,
        height: 0.05,
    }
}

// ---------- list ----------

/// 无注释的页面：list 返回空。
#[test]
fn list_empty_returns_empty() {
    let bytes = make_text_pdf("Hello");
    let result = list_annotations_logic(&bytes, Some(0)).unwrap();
    assert!(result.is_empty(), "新页面应无注释");
}

/// 列所有页：None 表示全文档。
#[test]
fn list_all_pages() {
    let bytes = make_text_pdf("Hello");
    let result = list_annotations_logic(&bytes, None).unwrap();
    assert_eq!(result.len(), 0);
}

/// 越界页码返回错误。
#[test]
fn list_page_out_of_range() {
    let bytes = make_text_pdf("Hello");
    let result = list_annotations_logic(&bytes, Some(99));
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "page_out_of_range");
}

// ---------- add 高亮 ----------

/// 添加高亮后：list 能找到 1 个 Highlight。
#[test]
fn add_highlight_appears_in_list() {
    let bytes = make_text_pdf("Highlight");
    let opts = AddAnnotationOpts {
        kind: AnnotationKind::Highlight,
        region: region(),
        contents: "key".to_string(),
        color: "#ffff00".into(),
        opacity: 1.0,
    };
    let new_bytes = add_annotation_logic(&bytes, 0, &opts).unwrap();
    let list = list_annotations_logic(&new_bytes, Some(0)).unwrap();
    assert_eq!(list.len(), 1);
    assert!(matches!(list[0].kind, AnnotationKind::Highlight));
}

/// 高亮 bounds 落在 page 内。
#[test]
fn highlight_bounds_within_page() {
    let bytes = make_text_pdf("A");
    let opts = AddAnnotationOpts {
        kind: AnnotationKind::Highlight,
        region: region(),
        contents: "".into(),
        color: "#ffff00".into(),
        opacity: 1.0,
    };
    let new_bytes = add_annotation_logic(&bytes, 0, &opts).unwrap();
    let list = list_annotations_logic(&new_bytes, Some(0)).unwrap();
    let a = &list[0];
    // A4 ≈ 595 x 842
    assert!(a.left >= 0.0 && a.right <= 600.0);
    assert!(a.bottom >= 0.0 && a.top <= 850.0);
    assert!(a.left < a.right && a.bottom < a.top, "rect 有效");
}

// ---------- add 多种类型 ----------

/// 添加下划线：返回 ok，list 找到 1 个 Underline。
#[test]
fn add_underline_ok() {
    let bytes = make_text_pdf("A");
    let opts = AddAnnotationOpts {
        kind: AnnotationKind::Underline,
        region: region(),
        contents: "".into(),
        color: "#ff0000".into(),
        opacity: 1.0,
    };
    let new_bytes = add_annotation_logic(&bytes, 0, &opts).unwrap();
    let list = list_annotations_logic(&new_bytes, Some(0)).unwrap();
    assert_eq!(list.len(), 1);
    assert!(matches!(list[0].kind, AnnotationKind::Underline));
}

/// 添加删除线：返回 ok。
#[test]
fn add_strikeout_ok() {
    let bytes = make_text_pdf("A");
    let opts = AddAnnotationOpts {
        kind: AnnotationKind::Strikeout,
        region: region(),
        contents: "".into(),
        color: "#00ff00".into(),
        opacity: 1.0,
    };
    let new_bytes = add_annotation_logic(&bytes, 0, &opts).unwrap();
    let list = list_annotations_logic(&new_bytes, Some(0)).unwrap();
    assert_eq!(list.len(), 1);
    assert!(matches!(list[0].kind, AnnotationKind::Strikeout));
}

/// 添加便签：返回 ok。
#[test]
fn add_sticky_note_ok() {
    let bytes = make_text_pdf("A");
    let opts = AddAnnotationOpts {
        kind: AnnotationKind::StickyNote,
        region: region(),
        contents: "TODO: review this".into(),
        color: "#ffff00".into(),
        opacity: 1.0,
    };
    let new_bytes = add_annotation_logic(&bytes, 0, &opts).unwrap();
    let list = list_annotations_logic(&new_bytes, Some(0)).unwrap();
    assert_eq!(list.len(), 1);
    assert!(matches!(list[0].kind, AnnotationKind::StickyNote));
    assert_eq!(list[0].contents, "TODO: review this");
}

/// 添加自由文本：返回 ok。
#[test]
fn add_free_text_ok() {
    let bytes = make_text_pdf("A");
    let opts = AddAnnotationOpts {
        kind: AnnotationKind::FreeText,
        region: region(),
        contents: "annotation text".into(),
        color: "#000000".into(),
        opacity: 1.0,
    };
    let new_bytes = add_annotation_logic(&bytes, 0, &opts).unwrap();
    let list = list_annotations_logic(&new_bytes, Some(0)).unwrap();
    assert_eq!(list.len(), 1);
    assert!(matches!(list[0].kind, AnnotationKind::FreeText));
}

/// 添加矩形：返回 ok。
#[test]
fn add_square_ok() {
    let bytes = make_text_pdf("A");
    let opts = AddAnnotationOpts {
        kind: AnnotationKind::Square,
        region: region(),
        contents: "box".into(),
        color: "#0000ff".into(),
        opacity: 1.0,
    };
    let new_bytes = add_annotation_logic(&bytes, 0, &opts).unwrap();
    let list = list_annotations_logic(&new_bytes, Some(0)).unwrap();
    assert_eq!(list.len(), 1);
    assert!(matches!(list[0].kind, AnnotationKind::Square));
}

// ---------- 多注释 / 多页 ----------

/// 同一页面添加多个注释：list 全部返回。
#[test]
fn add_multiple_annotations() {
    let bytes = make_text_pdf("A");
    let mut current = bytes;
    for i in 0..5 {
        let opts = AddAnnotationOpts {
            kind: AnnotationKind::Highlight,
            region: RegionSpec {
                left: 0.1 + (i as f32) * 0.05,
                top: 0.1,
                width: 0.04,
                height: 0.05,
            },
            contents: format!("#{}", i),
            color: "#ffff00".into(),
            opacity: 1.0,
        };
        current = add_annotation_logic(&current, 0, &opts).unwrap();
    }
    let list = list_annotations_logic(&current, Some(0)).unwrap();
    assert_eq!(list.len(), 5);
}

/// 多页 PDF：list(Some(p)) 只返回该页。
#[test]
fn list_filters_to_page() {
    let pdfium = pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    doc.pages_mut().create_page_at_index(size, 1).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let opts = AddAnnotationOpts {
        kind: AnnotationKind::Highlight,
        region: region(),
        contents: "p0".into(),
        color: "#ffff00".into(),
        opacity: 1.0,
    };
    let new_bytes = add_annotation_logic(&bytes, 0, &opts).unwrap();

    let list = list_annotations_logic(&new_bytes, Some(0)).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].page_index, 0);

    let list = list_annotations_logic(&new_bytes, Some(1)).unwrap();
    assert!(list.is_empty(), "page 1 应该无注释");
}

// ---------- 删除 ----------

/// 删除注释：list 数量减 1。
#[test]
fn delete_annotation_removes_from_list() {
    let bytes = make_text_pdf("A");
    let opts = AddAnnotationOpts {
        kind: AnnotationKind::Highlight,
        region: region(),
        contents: "".into(),
        color: "#ffff00".into(),
        opacity: 1.0,
    };
    let after_add = add_annotation_logic(&bytes, 0, &opts).unwrap();
    let list = list_annotations_logic(&after_add, Some(0)).unwrap();
    assert_eq!(list.len(), 1);

    let after_del = delete_annotation_logic(&after_add, 0, 0).unwrap();
    let list = list_annotations_logic(&after_del, Some(0)).unwrap();
    assert!(list.is_empty(), "删除后 list 应为空");
}

/// 删除越界 annotation：返回 AnnotationOutOfRange。
#[test]
fn delete_annotation_out_of_range() {
    let bytes = make_text_pdf("A");
    let result = delete_annotation_logic(&bytes, 0, 99);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "annotation_out_of_range");
}

/// 删除 0 注释时调用删除：返回 AnnotationOutOfRange。
#[test]
fn delete_from_empty_page() {
    let bytes = make_text_pdf("A");
    let result = delete_annotation_logic(&bytes, 0, 0);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "annotation_out_of_range");
}

/// 删除后 PDF 仍然能被 pdfium 打开（结构完整）。
#[test]
fn delete_keeps_pdf_valid() {
    let bytes = make_text_pdf("Hello");
    let opts = AddAnnotationOpts {
        kind: AnnotationKind::Highlight,
        region: region(),
        contents: "".into(),
        color: "#ffff00".into(),
        opacity: 1.0,
    };
    let after_add = add_annotation_logic(&bytes, 0, &opts).unwrap();
    let after_del = delete_annotation_logic(&after_add, 0, 0).unwrap();
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&after_del, None).unwrap();
    assert_eq!(doc.pages().len(), 1);
}

// ---------- 错误处理 ----------

/// 添加注释页码越界：返回 PageOutOfRange。
#[test]
fn add_annotation_page_out_of_range() {
    let bytes = make_text_pdf("A");
    let opts = AddAnnotationOpts {
        kind: AnnotationKind::Highlight,
        region: region(),
        contents: "".into(),
        color: "#ffff00".into(),
        opacity: 1.0,
    };
    let result = add_annotation_logic(&bytes, 99, &opts);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "page_out_of_range");
}

/// 删除注释页码越界：返回 PageOutOfRange。
#[test]
fn delete_annotation_page_out_of_range() {
    let bytes = make_text_pdf("A");
    let result = delete_annotation_logic(&bytes, 99, 0);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "page_out_of_range");
}

/// 损坏文件：list 返回错误。
#[test]
fn list_corrupted_returns_error() {
    let garbage = b"not a pdf";
    let result = list_annotations_logic(garbage, Some(0));
    assert!(result.is_err());
}