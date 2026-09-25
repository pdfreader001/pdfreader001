//! M6 安全模块测试
//!
//! 由于 pdfium-render 不支持设置密码（只能读取），我们主要测试：
//! - 未加密文档的安全状态读取（get_security_status_logic）
//! - 明文副本导出：重新序列化后的 bytes 能被正常打开
//! - 边界条件：损坏文件返回错误

use std::fs;
use std::path::PathBuf;

use pdfe_lib::security::get_security_status_logic;
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

// ---------- get_security_status_logic ----------

/// 未加密文档应返回 Unprotected，且权限基本全开。
#[test]
fn unprotected_doc_status_is_open() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let status = get_security_status_logic(&doc);
    assert_eq!(status.handler_revision, "Unprotected");
    assert!(status.can_print_high_quality);
    assert!(status.can_modify_document);
    assert!(status.can_extract_text_and_graphics);
    assert!(status.can_add_annotations);
    assert!(status.can_fill_form_fields);
    assert!(status.can_assemble_document);
    assert!(status.can_create_new_form_fields);
}

/// handler_revision 的值必须是已知枚举之一。
#[test]
fn handler_revision_is_known_variant() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let status = get_security_status_logic(&doc);
    let known = [
        "Unprotected",
        "Revision2",
        "Revision3",
        "Revision4",
        "Unknown",
    ];
    assert!(
        known.contains(&status.handler_revision.as_str()),
        "handler_revision = {}",
        status.handler_revision
    );
}

/// SecurityStatus 结构体 9 个字段都能正常访问。
#[test]
fn security_status_all_fields_accessible() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let status = get_security_status_logic(&doc);
    // 编译期保证字段存在且类型正确
    let _: &str = &status.handler_revision;
    let _: bool = status.can_print_high_quality;
    let _: bool = status.can_print_low_quality;
    let _: bool = status.can_modify_document;
    let _: bool = status.can_extract_text_and_graphics;
    let _: bool = status.can_add_annotations;
    let _: bool = status.can_fill_form_fields;
    let _: bool = status.can_assemble_document;
    let _: bool = status.can_create_new_form_fields;
}

/// 打印质量：高质量和低质量不能同时为 true。
#[test]
fn print_quality_mutually_exclusive() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let status = get_security_status_logic(&doc);
    // 未加密文档：高质量 true，低质量 false
    if status.handler_revision == "Unprotected" {
        assert!(status.can_print_high_quality);
        assert!(!status.can_print_low_quality);
    } else {
        // 加密文档也不能同时为 true
        assert!(
            !(status.can_print_high_quality && status.can_print_low_quality),
            "cannot have both high and low quality print"
        );
    }
}

/// 损坏文件无法加载，load_pdf_from_byte_slice 应返回错误。
#[test]
fn corrupted_file_fails_to_load() {
    let bytes = b"this is definitely not a valid pdf file";
    let result = pdfium().load_pdf_from_byte_slice(bytes, None);
    assert!(result.is_err(), "corrupted bytes should fail to load");
}

// ---------- 明文副本导出（等价行为）----------

/// 模拟 export_plain_copy：重新序列化后的 bytes 应该是有效 PDF 且能正常打开。
#[test]
fn plain_copy_reserialization_is_valid() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page_count_before = doc.pages().len();
    let first_page_text_before = doc.pages().get(0).unwrap().text().unwrap().all();

    // 重新序列化（去掉任何加密字典）
    let plain_bytes = doc.save_to_bytes().unwrap();
    assert!(
        !plain_bytes.is_empty(),
        "reserialized bytes should not be empty"
    );

    // 重新加载验证
    let doc2 = pdfium()
        .load_pdf_from_byte_slice(&plain_bytes, None)
        .unwrap();
    assert_eq!(doc2.pages().len(), page_count_before);

    // 文本内容应该一致
    let first_page_text_after = doc2.pages().get(0).unwrap().text().unwrap().all();
    assert_eq!(first_page_text_before, first_page_text_after);
}

/// 明文导出后安全状态仍为 Unprotected。
#[test]
fn plain_copy_security_status_is_unprotected() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let plain_bytes = doc.save_to_bytes().unwrap();
    let doc2 = pdfium()
        .load_pdf_from_byte_slice(&plain_bytes, None)
        .unwrap();
    let status = get_security_status_logic(&doc2);
    assert_eq!(status.handler_revision, "Unprotected");
}

// ---------- SecurityStatus 序列化（camelCase 验证）----------

/// SecurityStatus 序列化为 JSON 时字段名应为 camelCase。
#[test]
fn security_status_serializes_camel_case() {
    use serde_json::Value;

    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let status = get_security_status_logic(&doc);
    let json = serde_json::to_string(&status).unwrap();
    let v: Value = serde_json::from_str(&json).unwrap();

    assert!(v.get("handlerRevision").is_some(), "handlerRevision");
    assert!(
        v.get("canPrintHighQuality").is_some(),
        "canPrintHighQuality"
    );
    assert!(v.get("canPrintLowQuality").is_some(), "canPrintLowQuality");
    assert!(v.get("canModifyDocument").is_some(), "canModifyDocument");
    assert!(
        v.get("canExtractTextAndGraphics").is_some(),
        "canExtractTextAndGraphics"
    );
    assert!(v.get("canAddAnnotations").is_some(), "canAddAnnotations");
    assert!(v.get("canFillFormFields").is_some(), "canFillFormFields");
    assert!(
        v.get("canAssembleDocument").is_some(),
        "canAssembleDocument"
    );
    assert!(
        v.get("canCreateNewFormFields").is_some(),
        "canCreateNewFormFields"
    );
}

/// SecurityStatus 的 bool 字段值在 JSON 中正确传递。
#[test]
fn security_status_bool_values_correct() {
    use serde_json::Value;

    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let doc = pdfium().load_pdf_from_byte_slice(&bytes, None).unwrap();
    let status = get_security_status_logic(&doc);
    let json = serde_json::to_string(&status).unwrap();
    let v: Value = serde_json::from_str(&json).unwrap();

    assert_eq!(
        v["canPrintHighQuality"].as_bool().unwrap(),
        status.can_print_high_quality
    );
    assert_eq!(
        v["canModifyDocument"].as_bool().unwrap(),
        status.can_modify_document
    );
    assert_eq!(
        v["canExtractTextAndGraphics"].as_bool().unwrap(),
        status.can_extract_text_and_graphics
    );
    assert_eq!(
        v["handlerRevision"].as_str().unwrap(),
        status.handler_revision
    );
}
