//! M6：文档安全状态查看、明文副本导出与加密副本导出
//!
//! pdfium-render 只能读取加密状态、不能写入加密（`save_to_writer` 内部把 flags 硬编码为 0），
//! 因此本模块能力：
//! - 读取并展示安全处理版本
//! - 读取并展示 7 项权限
//! - 另存为明文副本（解密后的等价物重新序列化 → 移除加密）
//! - 另存为加密副本（pdfium 产出明文 → lopdf 载入 → EncryptionState → 重序列化）
//!
//! 注意：加密只作用于写盘的副本，内存中的 bytes 始终保持明文，
//! 否则 get_metadata / save_document / 撤销重做 / 渲染都会因需要密码而失效。

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::encryption::{
    crypt_filters::{Aes128CryptFilter, CryptFilter},
    EncryptionState, EncryptionVersion, Permissions,
};
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::document::{pdfium as get_pdfium, AppState, DocumentInfo};
use crate::error::{AppError, AppResult};
use crate::pages::commit_and_return;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityStatus {
    /// "Unprotected" / "Revision2" / "Revision3" / "Revision4" / "Unknown"
    pub handler_revision: String,
    /// 7 项权限（pdfium 通过 bits 推导）
    pub can_print_high_quality: bool,
    pub can_print_low_quality: bool,
    pub can_modify_document: bool,
    pub can_extract_text_and_graphics: bool,
    pub can_add_annotations: bool,
    pub can_fill_form_fields: bool,
    pub can_assemble_document: bool,
    pub can_create_new_form_fields: bool,
}

fn map_revision(rev: &PdfSecurityHandlerRevision) -> &'static str {
    match rev {
        PdfSecurityHandlerRevision::Unprotected => "Unprotected",
        PdfSecurityHandlerRevision::Revision2 => "Revision2",
        PdfSecurityHandlerRevision::Revision3 => "Revision3",
        PdfSecurityHandlerRevision::Revision4 => "Revision4",
    }
}

/// 纯函数版本：从已加载的 PdfDocument 提取安全状态。
pub fn get_security_status_logic(doc: &PdfDocument) -> SecurityStatus {
    let perms = doc.permissions();
    let rev = perms
        .security_handler_revision()
        .ok()
        .as_ref()
        .map(map_revision)
        .unwrap_or("Unknown");
    // 高质量打印 / 低质量打印：perms 同时只可能有一个为 true
    let hi = perms.can_print_high_quality().unwrap_or(true);
    let lo = perms.can_print_only_low_quality().unwrap_or(false);
    SecurityStatus {
        handler_revision: rev.to_string(),
        can_print_high_quality: hi,
        can_print_low_quality: lo,
        can_modify_document: perms.can_modify_document_content().unwrap_or(true),
        can_extract_text_and_graphics: perms.can_extract_text_and_graphics().unwrap_or(true),
        can_add_annotations: perms.can_add_or_modify_text_annotations().unwrap_or(true),
        can_fill_form_fields: perms
            .can_fill_existing_interactive_form_fields()
            .unwrap_or(true),
        can_assemble_document: perms.can_assemble_document().unwrap_or(true),
        can_create_new_form_fields: perms
            .can_create_new_interactive_form_fields()
            .unwrap_or(true),
    }
}

#[tauri::command]
pub async fn get_security_status(
    state: State<'_, AppState>,
    doc_id: u64,
) -> AppResult<SecurityStatus> {
    let pdfium = get_pdfium();
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    let doc = pdfium.load_pdf_from_byte_slice(&entry.bytes, None)?;
    Ok(get_security_status_logic(&doc))
}

/// 将当前文档以"明文副本"形式保存到新路径。
/// 实际行为：通过 pdfium 重新序列化为 bytes（无密码、无加密字典），
/// 再用 Rust 标准库写入目标文件。
#[tauri::command]
pub async fn export_plain_copy(
    state: State<'_, AppState>,
    doc_id: u64,
    output_path: String,
) -> AppResult<String> {
    let pdfium = get_pdfium();
    let bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let doc = pdfium.load_pdf_from_byte_slice(&entry.bytes, None)?;
        doc.save_to_bytes()?
    };
    std::fs::write(&output_path, &bytes)
        .map_err(|e| AppError::Io(e))?;
    Ok(output_path)
}

// ------ 加密副本导出（AES-128 / Revision 4） ------

/// 加密导出参数（前端传入）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptOptions {
    /// 打开密码。为空则无需密码即可打开文档（权限限制仍然生效）。
    pub user_password: String,
    /// 权限密码。为空时回落到打开密码。
    pub owner_password: String,
    pub allow_print: bool,
    pub allow_copy: bool,
    pub allow_modify: bool,
    pub allow_annotate: bool,
}

impl EncryptOptions {
    fn effective_owner_password(&self) -> &str {
        if self.owner_password.is_empty() {
            &self.user_password
        } else {
            &self.owner_password
        }
    }
}

/// 由 4 个开关组装 PDF 权限位。
fn build_permissions(opts: &EncryptOptions) -> Permissions {
    let mut perms = Permissions::empty();
    if opts.allow_print {
        perms.insert(Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY);
    }
    if opts.allow_copy {
        perms.insert(Permissions::COPYABLE | Permissions::COPYABLE_FOR_ACCESSIBILITY);
    }
    if opts.allow_modify {
        perms.insert(Permissions::MODIFIABLE);
    }
    if opts.allow_annotate {
        perms.insert(Permissions::ANNOTABLE);
    }
    perms
}

/// 纯函数：把明文 PDF 字节加密为 AES-128（V4 / Revision 4）加密 PDF。
///
/// 链路：lopdf 载入明文 → `EncryptionState`（V4）→ 加密所有字符串与流 → 重序列化。
pub fn encrypt_pdf_bytes_logic(plain: &[u8], opts: &EncryptOptions) -> AppResult<Vec<u8>> {
    let owner_password = opts.effective_owner_password();
    if opts.user_password.is_empty() && owner_password.is_empty() {
        return Err(AppError::PasswordEmpty);
    }

    let mut doc = lopdf::Document::load_mem(plain)
        .map_err(|e| AppError::PdfEncryptFailed { detail: e.to_string() })?;

    // Standard security handler 的加密过滤器，名字必须与 stream/string filter 一致。
    let mut crypt_filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
    crypt_filters.insert(b"StdCF".to_vec(), Arc::new(Aes128CryptFilter));

    let state = EncryptionState::try_from(EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password,
        user_password: &opts.user_password,
        permissions: build_permissions(opts),
    })
    .map_err(|e| AppError::PdfEncryptFailed { detail: e.to_string() })?;

    doc.encrypt(&state)
        .map_err(|e| AppError::PdfEncryptFailed { detail: e.to_string() })?;

    // 加密文档会跳过 object streams（流内容已加密、文件密钥已不可得），
    // 每个对象单独序列化，xref 流不参与加密。
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| AppError::PdfEncryptFailed { detail: e.to_string() })?;
    Ok(out)
}

/// 把当前文档以「加密副本」形式保存到新路径。
///
/// 只写盘，不改内存 bytes（内存中始终为明文，供渲染/编辑/撤销重做使用）。
#[tauri::command]
pub async fn export_encrypted_copy(
    state: State<'_, AppState>,
    doc_id: u64,
    output_path: String,
    options: EncryptOptions,
) -> AppResult<String> {
    let pdfium = get_pdfium();
    let plain = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let doc = pdfium.load_pdf_from_byte_slice(&entry.bytes, None)?;
        doc.save_to_bytes()?
    };
    let encrypted = encrypt_pdf_bytes_logic(&plain, &options)?;
    std::fs::write(&output_path, &encrypted).map_err(AppError::Io)?;
    Ok(output_path)
}

/// 触发一次"另存为"：让用户选保存路径，返回该路径；调用方自行用 save_document 写入。
#[allow(dead_code)]
#[tauri::command]
pub async fn touch_save_marker(state: State<'_, AppState>, doc_id: u64) -> AppResult<bool> {
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    let pdfium = get_pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&entry.bytes, None)?;
    let pages = doc.pages().len();
    let _ = pages;
    Ok(true)
}

/// 把当前文档以明文 bytes 重新载入内存（去掉原密码/加密字典）。
#[tauri::command]
pub async fn reload_plain(
    state: State<'_, AppState>,
    doc_id: u64,
) -> AppResult<DocumentInfo> {
    // 把当前 entry 的 bytes 重写为明文（去掉密码/加密字典）
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let doc = pdfium.load_pdf_from_byte_slice(&entry.bytes, None)?;
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}
#[cfg(test)]
mod tests {
    //! Unit tests for security status mapping.
    use super::*;
    use pdfium_render::prelude::PdfSecurityHandlerRevision;

    /// map_revision: known variants round-trip to stable strings.
    #[test]
    fn map_revision_known_variants() {
        assert_eq!(map_revision(&PdfSecurityHandlerRevision::Unprotected), "Unprotected");
        assert_eq!(map_revision(&PdfSecurityHandlerRevision::Revision2), "Revision2");
        assert_eq!(map_revision(&PdfSecurityHandlerRevision::Revision3), "Revision3");
        assert_eq!(map_revision(&PdfSecurityHandlerRevision::Revision4), "Revision4");
    }

    /// SecurityStatus: default-constructed status serializes all 8 bool fields.
    /// (Useful as a sanity check that the schema hasn't drifted.)
    #[test]
    fn security_status_default_serializes_all_fields() {
        let s = SecurityStatus {
            handler_revision: "Revision3".to_string(),
            can_print_high_quality: true,
            can_print_low_quality: false,
            can_modify_document: true,
            can_extract_text_and_graphics: true,
            can_add_annotations: true,
            can_fill_form_fields: true,
            can_assemble_document: false,
            can_create_new_form_fields: false,
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["handlerRevision"], "Revision3");
        assert_eq!(v["canPrintHighQuality"], true);
        assert_eq!(v["canPrintLowQuality"], false);
        assert_eq!(v["canModifyDocument"], true);
        assert_eq!(v["canExtractTextAndGraphics"], true);
        assert_eq!(v["canAddAnnotations"], true);
        assert_eq!(v["canFillFormFields"], true);
        assert_eq!(v["canAssembleDocument"], false);
        assert_eq!(v["canCreateNewFormFields"], false);
    }

    fn opts(user: &str, owner: &str, all: bool) -> EncryptOptions {
        EncryptOptions {
            user_password: user.to_string(),
            owner_password: owner.to_string(),
            allow_print: all,
            allow_copy: all,
            allow_modify: all,
            allow_annotate: all,
        }
    }

    /// 权限密码为空时回落到打开密码。
    #[test]
    fn owner_password_falls_back_to_user_password() {
        assert_eq!(opts("u", "", true).effective_owner_password(), "u");
        assert_eq!(opts("u", "o", true).effective_owner_password(), "o");
        assert_eq!(opts("", "o", true).effective_owner_password(), "o");
    }

    /// 4 个开关与权限位的映射。
    #[test]
    fn build_permissions_follows_toggles() {
        assert_eq!(build_permissions(&opts("u", "", false)).bits(), 0);

        let none = build_permissions(&opts("u", "", false));
        assert!(!none.contains(Permissions::PRINTABLE));
        assert!(!none.contains(Permissions::COPYABLE));
        assert!(!none.contains(Permissions::MODIFIABLE));
        assert!(!none.contains(Permissions::ANNOTABLE));

        let all = build_permissions(&opts("u", "", true));
        assert!(all.contains(Permissions::PRINTABLE));
        assert!(all.contains(Permissions::PRINTABLE_IN_HIGH_QUALITY));
        assert!(all.contains(Permissions::COPYABLE));
        assert!(all.contains(Permissions::MODIFIABLE));
        assert!(all.contains(Permissions::ANNOTABLE));

        let print_only = build_permissions(&EncryptOptions {
            user_password: "u".into(),
            owner_password: String::new(),
            allow_print: true,
            allow_copy: false,
            allow_modify: false,
            allow_annotate: false,
        });
        assert!(print_only.contains(Permissions::PRINTABLE));
        assert!(!print_only.contains(Permissions::MODIFIABLE));
    }

    /// 两个密码都为空时，在解析 PDF 之前就应报 password_empty。
    #[test]
    fn encrypt_rejects_empty_passwords() {
        let err = encrypt_pdf_bytes_logic(b"%PDF-1.4", &opts("", "", true)).unwrap_err();
        assert_eq!(err.code(), "password_empty");
        assert!(serde_json::to_value(&err).unwrap().get("args").is_none());
    }

    /// 非法 PDF 内容 → pdf_encrypt_failed（带 detail）。
    #[test]
    fn encrypt_reports_parse_failure() {
        let err = encrypt_pdf_bytes_logic(b"not a pdf at all", &opts("u", "", true)).unwrap_err();
        assert_eq!(err.code(), "pdf_encrypt_failed");
        assert!(serde_json::to_value(&err).unwrap()["args"]["detail"].is_string());
    }
}
