//! 加密副本导出（M6 安全补强）集成测试。
//!
//! 这些测试直接调用命令背后的 pub 纯函数 `encrypt_pdf_bytes_logic`，
//! 在真实 pdfium 运行路径下验证：
//! - 加密产物能被 pdfium 用正确打开密码重新打开
//! - 错误密码 / 缺省密码无法打开
//! - 权限开关映射为 pdfium 可读出的权限矩阵
//! - 仅设权限密码（打开密码留空）时无需密码即可打开，但权限仍受限

use pdfium_render::prelude::*;
use pdfe_lib::security::{encrypt_pdf_bytes_logic, EncryptOptions};

fn pdfium<'a>() -> &'a Pdfium {
    pdfe_lib::pdfium()
}

/// 生成一个单页空白 PDF（作为加密的明文输入）。
fn make_pdf() -> Vec<u8> {
    let pdfium = pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let size = PdfPagePaperSize::a4();
    doc.pages_mut().create_page_at_index(size, 0).unwrap();
    doc.save_to_bytes().unwrap()
}

fn options(user: &str, owner: &str) -> EncryptOptions {
    EncryptOptions {
        user_password: user.to_string(),
        owner_password: owner.to_string(),
        allow_print: true,
        allow_copy: false,
        allow_modify: false,
        allow_annotate: false,
    }
}

/// 打开密码 + 权限密码：必须用正确密码才能打开。
#[test]
fn encrypt_requires_correct_open_password() {
    let encrypted = encrypt_pdf_bytes_logic(&make_pdf(), &options("secret", "owner")).unwrap();
    let pdfium = pdfium();

    let doc = pdfium
        .load_pdf_from_byte_slice(&encrypted, Some("secret"))
        .expect("correct password should open the document");
    assert_eq!(doc.pages().len(), 1);

    let err = pdfium
        .load_pdf_from_byte_slice(&encrypted, Some("nope"))
        .err()
        .expect("wrong password must fail");
    assert_eq!(pdfe_lib::error::AppError::from(err).code(), "password");

    let err = pdfium
        .load_pdf_from_byte_slice(&encrypted, None)
        .err()
        .expect("missing password must fail");
    assert_eq!(pdfe_lib::error::AppError::from(err).code(), "password");
}

/// 权限开关能一一映射到 pdfium 读出的权限矩阵。
///
/// 打开密码与权限密码必须不同：PDF 标准安全处理器在两者相同时会把用户认证为所有者，
/// pdfium 随之放行全部权限，导致权限断言失去意义。
#[test]
fn encrypt_maps_permission_toggles() {
    let pdfium = pdfium();

    // 只允许打印，其余 3 项在 pdfium 侧也必须读出 false。
    let restricted = encrypt_pdf_bytes_logic(&make_pdf(), &options("pw", "owner-pw")).unwrap();
    let doc = pdfium
        .load_pdf_from_byte_slice(&restricted, Some("pw"))
        .unwrap();
    let perms = doc.permissions();

    assert_eq!(
        perms.security_handler_revision().unwrap(),
        PdfSecurityHandlerRevision::Revision4
    );
    assert_eq!(perms.can_print_high_quality().unwrap(), true);
    assert_eq!(perms.can_modify_document_content().unwrap(), false);
    assert_eq!(perms.can_extract_text_and_graphics().unwrap(), false);
    assert_eq!(perms.can_add_or_modify_text_annotations().unwrap(), false);

    // 全部放开的对照：4 项在 pdfium 侧都应读出 true。
    let open = encrypt_pdf_bytes_logic(
        &make_pdf(),
        &EncryptOptions {
            allow_copy: true,
            allow_modify: true,
            allow_annotate: true,
            ..options("pw", "owner-pw")
        },
    )
    .unwrap();
    let doc = pdfium.load_pdf_from_byte_slice(&open, Some("pw")).unwrap();
    let perms = doc.permissions();

    assert_eq!(perms.can_print_high_quality().unwrap(), true);
    assert_eq!(perms.can_modify_document_content().unwrap(), true);
    assert_eq!(perms.can_extract_text_and_graphics().unwrap(), true);
    assert_eq!(perms.can_add_or_modify_text_annotations().unwrap(), true);
}

/// 仅设权限密码：无需密码即可打开，但权限仍然受限；权限密码同样可打开。
#[test]
fn encrypt_with_owner_password_only() {
    let encrypted = encrypt_pdf_bytes_logic(&make_pdf(), &options("", "owner")).unwrap();
    let pdfium = pdfium();

    let doc = pdfium
        .load_pdf_from_byte_slice(&encrypted, None)
        .expect("empty open password should open without a password");
    assert_eq!(
        doc.permissions().can_extract_text_and_graphics().unwrap(),
        false
    );

    assert!(pdfium
        .load_pdf_from_byte_slice(&encrypted, Some("owner"))
        .is_ok());
}
