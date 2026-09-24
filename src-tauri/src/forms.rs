//! P5+P6：PDF 表单（AcroForm）
//!
//! 能力（pdfium-render 0.8.37 实际支持）：
//! - 列出表单字段名和当前值（通过 PdfForm::field_values(&pages)）
//! - 设置表单字段值（Text / Checkbox）—— 待走 Widget annotation mutable 路径
//!
//! 当前完成：list_form_fields_logic（列出字段名 + 当前值）
//! 待做：set_form_field_value_logic（需要 mutable Widget annotation 路径）

use serde::{Deserialize, Serialize};

use crate::document::pdfium;
use crate::error::{AppError, AppResult};
use crate::pages::load_doc;

/// 表单字段描述（最小版本：只有 name + value）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormFieldInfo {
    pub name: String,
    pub value: String,
}

/// 设置表单字段值
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFormFieldOpts {
    pub name: String,
    pub value: String,
}

// ============================================================================
// 纯函数
// ============================================================================

/// 纯函数版本：列出所有表单字段
///
/// pdfium-render 0.8.37 的 `PdfForm::field_values(&pages)` 返回
/// `HashMap<String, Option<String>>`，直接是字段名到值的 Map。
/// 只暴露 name + value（版本 1 简化）。
pub fn list_form_fields_logic(bytes: &[u8]) -> AppResult<Vec<FormFieldInfo>> {
    let pdfium_inst = pdfium();
    let doc = load_doc(pdfium_inst, bytes)?;

    let form = match doc.form() {
        Some(f) => f,
        None => return Ok(vec![]), // 无 AcroForm → 空列表
    };

    let pages = doc.pages();
    let values_map = form.field_values(&pages);

    let mut result: Vec<FormFieldInfo> = values_map
        .into_iter()
        .map(|(name, value)| FormFieldInfo {
            name,
            value: value.unwrap_or_default(),
        })
        .collect();

    // 稳定排序（按 name）
    result.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(result)
}

/// 纯函数版本：设置单个表单字段值，返回修改后的 bytes
///
/// pdfium-render 0.8.37 的字段写 API 有共享引用限制，
/// 必须通过 **Widget annotation 的 mutable 路径** 实现。
/// 当前占位版本返回 NotImplemented。
pub fn set_form_field_value_logic(
    _bytes: &[u8],
    _opts: &SetFormFieldOpts,
) -> AppResult<Vec<u8>> {
    // pdfium-render 0.8.37 的字段写 API 有共享引用限制。
    // 当前占位版本返回错误；后续版本通过 Widget annotation mutable 路径实现。
    Err(AppError::NotFound)
}

// ============================================================================
// Tauri commands
// ============================================================================

#[tauri::command]
pub async fn list_form_fields(
    state: tauri::State<'_, crate::document::AppState>,
    doc_id: u64,
) -> AppResult<Vec<FormFieldInfo>> {
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    list_form_fields_logic(&entry.bytes)
}

#[tauri::command]
pub async fn set_form_field_value(
    _state: tauri::State<'_, crate::document::AppState>,
    _doc_id: u64,
    _opts: SetFormFieldOpts,
) -> AppResult<crate::document::DocumentInfo> {
    // pdfium-render 0.8.37 的字段写 API 需要 Widget annotation mutable 路径，
    // 当前版本暂未实现。返回 NotFound 占位。
    Err(AppError::NotFound)
}
