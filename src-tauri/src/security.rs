//! M6：文档安全状态查看与明文副本导出
//!
//! pdfium-render 不支持修改文档的加密设置，
//! 因此本模块能力限于：
//! - 读取并展示安全处理版本
//! - 读取并展示 7 项权限
//! - 另存为明文副本（解密后的等价物重新序列化 → 移除加密）

use pdfium_render::prelude::*;
use serde::Serialize;
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

#[tauri::command]
pub async fn get_security_status(
    state: State<'_, AppState>,
    doc_id: u64,
) -> AppResult<SecurityStatus> {
    let pdfium = get_pdfium();
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    let doc = pdfium.load_pdf_from_byte_slice(&entry.bytes, None)?;
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
    Ok(SecurityStatus {
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
    })
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