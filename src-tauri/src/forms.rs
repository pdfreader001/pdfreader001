//! P5+P6：PDF 表单（AcroForm）
//!
//! 能力（pdfium-render 0.8.37 实际支持）：
//! - 列出表单字段名和当前值（通过 PdfForm::field_values(&pages)）
//! - 设置表单字段值（Text / Checkbox）—— 通过 Widget annotation mutable 路径
//!
//! 写路径方案：
//!   doc.pages_mut() → page.annotations_mut() → annot.as_widget_annotation_mut()
//!     → widget.form_field_mut() → 按 name() 匹配 →
//!        as_text_field_mut().set_value(&str)
//!        as_checkbox_field_mut().set_checked(bool)

use serde::{Deserialize, Serialize};

use crate::document::{pdfium as get_pdfium, push_snapshot, AppState, DocumentInfo};
use crate::error::{AppError, AppResult};
use crate::pages::{commit_and_return, load_doc};

/// 表单字段类型（对应 pdfium-render 的 PdfFormFieldType）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FormFieldKind {
    Unknown,
    PushButton,
    Checkbox,
    RadioButton,
    ComboBox,
    ListBox,
    Text,
    Signature,
}

impl FormFieldKind {
    fn from_pdfium(k: pdfium_render::prelude::PdfFormFieldType) -> Self {
        use pdfium_render::prelude::PdfFormFieldType as F;
        match k {
            F::PushButton => Self::PushButton,
            F::Checkbox => Self::Checkbox,
            F::RadioButton => Self::RadioButton,
            F::ComboBox => Self::ComboBox,
            F::ListBox => Self::ListBox,
            F::Text => Self::Text,
            F::Signature => Self::Signature,
            F::Unknown => Self::Unknown,
        }
    }
}

/// 表单字段描述
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormFieldInfo {
    pub name: String,
    pub value: String,
    pub kind: FormFieldKind,
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
    use pdfium_render::prelude::PdfFormFieldCommon;

    let pdfium_inst = get_pdfium();
    let doc = load_doc(pdfium_inst, bytes)?;

    let form = match doc.form() {
        Some(f) => f,
        None => return Ok(vec![]), // 无 AcroForm → 空列表
    };

    let pages = doc.pages();
    let values_map = form.field_values(&pages);

    // 扫描所有页的 Widget annotation，拿到每个字段的 (name, kind)
    let mut kinds: Vec<(String, FormFieldKind)> = Vec::new();
    let total = doc.pages().len();
    for p_idx in 0..total {
        let page = doc.pages().get(p_idx)?;
        let annots = page.annotations();
        for i in 0..annots.len() {
            let mut annot = annots.get(i as usize)?;
            let widget = match annot.as_widget_annotation_mut() {
                Some(w) => w,
                None => continue,
            };
            let field = match widget.form_field() {
                Some(f) => f,
                None => continue,
            };
            if let Some(name) = PdfFormFieldCommon::name(field) {
                kinds.push((name, FormFieldKind::from_pdfium(field.field_type())));
            }
        }
    }

    let mut result: Vec<FormFieldInfo> = values_map
        .into_iter()
        .map(|(name, value)| {
            let kind = kinds
                .iter()
                .find(|(n, _)| n == &name)
                .map(|(_, k)| *k)
                .unwrap_or(FormFieldKind::Unknown);
            FormFieldInfo {
                name,
                value: value.unwrap_or_default(),
                kind,
            }
        })
        .collect();

    // 稳定排序（按 name）
    result.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(result)
}

/// 纯函数版本：设置单个表单字段值，返回修改后的 bytes
///
/// 遍历所有页的 Widget annotations，找到 name 匹配的目标字段。
/// Text 字段通过 `PdfFormTextField::set_value` 写入；
/// Checkbox 字段通过 `PdfFormCheckboxField::set_checked` 写入。
pub fn set_form_field_value_logic(
    bytes: &[u8],
    opts: &SetFormFieldOpts,
) -> AppResult<Vec<u8>> {
    use pdfium_render::prelude::PdfFormFieldCommon;

    let pdfium_inst = get_pdfium();
    let mut doc = load_doc(pdfium_inst, bytes)?;

    // 第一步：扫描所有 Widget annotation，定位目标字段的 (页索引, annotation 索引)
    let mut target: Option<(u16, u32, String)> = None;
    let total = doc.pages().len();
    for p_idx in 0..total {
        let page = doc.pages().get(p_idx)?;
        let annots = page.annotations();
        for i in 0..annots.len() {
            let mut annot = annots.get(i as usize)?;
            // 只考虑 Widget annotation
            let widget = match annot.as_widget_annotation_mut() {
                Some(w) => w,
                None => continue,
            };
            // 取出字段名（如果可读）
            let name_opt = widget
                .form_field()
                .and_then(|f| PdfFormFieldCommon::name(f));
            if let Some(name) = name_opt {
                if name == opts.name {
                    target = Some((p_idx, i as u32, name));
                    break;
                }
            }
            // widget/form_field 借用结束（每次循环结束自动 drop）
        }
        if target.is_some() {
            break;
        }
    }

    let (page_idx, annot_idx, _) = target.ok_or(AppError::NotFound)?;

    // 第二步：mutable 路径 — 拿到 form_field_mut 后按字段类型分支
    {
        let mut pages = doc.pages_mut();
        let mut page = pages.get(page_idx)?;
        let mut annots = page.annotations_mut();
        let mut annot = annots.get(annot_idx as usize)?;
        let mut widget = annot
            .as_widget_annotation_mut()
            .ok_or(AppError::NotFound)?;
        let field = widget.form_field_mut().ok_or(AppError::NotFound)?;

        // Text 字段：set_value
        if let Some(text_field) = field.as_text_field_mut() {
            text_field.set_value(&opts.value)?;
        }
        // Checkbox：value 是 "true"/"false" 或 "Yes"/"Off" → 解析布尔
        else if let Some(checkbox) = field.as_checkbox_field_mut() {
            let truthy = matches!(
                opts.value.to_ascii_lowercase().as_str(),
                "true" | "yes" | "on" | "1" | "checked"
            );
            checkbox.set_checked(truthy)?;
        } else {
            return Err(AppError::NotFound);
        }
    }

    Ok(doc.save_to_bytes()?)
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
    state: tauri::State<'_, AppState>,
    doc_id: u64,
    opts: SetFormFieldOpts,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        set_form_field_value_logic(&entry.bytes, &opts)?
    };
    commit_and_return(&state, doc_id, new_bytes)
}
