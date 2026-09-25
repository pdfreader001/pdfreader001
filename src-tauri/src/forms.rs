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

/// 扫描所有 Widget annotation，返回第一个 name 匹配的 (page_idx, annot_idx, kind)。
///
/// 内部使用 — 由 `set_form_field_value_logic` 第一步调用；提取成函数便于单元测试
/// 和未来扩展（ComboBox/ListBox 写路径）。
fn find_target_widget<'a>(
    doc: &pdfium_render::prelude::PdfDocument<'a>,
    field_name: &str,
) -> Option<(u16, u32, FormFieldKind)> {
    use pdfium_render::prelude::PdfFormFieldCommon;

    let total = doc.pages().len();
    for p_idx in 0..total {
        let page = doc.pages().get(p_idx).ok()?;
        let annots = page.annotations();
        for i in 0..annots.len() {
            let mut annot = annots.get(i as usize).ok()?;
            // Only Widget annotations carry form fields.
            let widget = annot.as_widget_annotation_mut()?;
            let resolved = widget.form_field().and_then(|f| {
                let name = PdfFormFieldCommon::name(f)?;
                let kind = FormFieldKind::from_pdfium(f.field_type());
                Some((name, kind))
            });
            if let Some((name, kind)) = resolved {
                if name == field_name {
                    return Some((p_idx, i as u32, kind));
                }
            }
        }
    }
    None
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
    let pdfium_inst = get_pdfium();
    let mut doc = load_doc(pdfium_inst, bytes)?;

    // 第一步：扫描所有 Widget annotation，定位目标字段
    let (page_idx, annot_idx, kind) = find_target_widget(&doc, &opts.name).ok_or(AppError::NotFound)?;

    // 第二步：mutable 路径 — 拿到 form_field_mut 后按字段类型分支
    {
        let pages = doc.pages_mut();
        let mut page = pages.get(page_idx)?;
        let annots = page.annotations_mut();
        let mut annot = annots.get(annot_idx as usize)?;
        let widget = annot
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
            // ComboBox / ListBox / RadioButton / Signature / PushButton 在 pdfium-render 0.8.37
            // 没有公开的 set_value API。
            return Err(AppError::FormFieldWriteUnsupported {
                kind: format!("{:?}", kind),
            });
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
    let _gate = crate::document::pdfium_gate();
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
    let _gate = crate::document::pdfium_gate();
    push_snapshot(&state, doc_id);
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        set_form_field_value_logic(&entry.bytes, &opts)?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

#[cfg(test)]
mod tests {
    //! Unit tests for the widget-target scanner extracted from
    //! `set_form_field_value_logic`.
    use super::*;
    use crate::document::pdfium;
    use pdfium_render::prelude::*;

    /// Hand-rolled PDF fixture builder (mirror of tests/forms.rs build_form_pdf_bytes).
    fn build_form_pdf_bytes() -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut xref_line = String::new();
        macro_rules! obj {
            ($n:expr, $body:expr) => {{
                xref_line.push_str(&format!("{:010} 00000 n \n", out.len()));
                out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", $n, $body).as_bytes());
            }};
        }
        out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
        obj!(1, "<< /Type /Pages /Kids [2 0 R] /Count 1 >>");
        obj!(2, "<< /Type /Page /Parent 1 0 R /MediaBox [0 0 792 612] /Annots [5 0 R 6 0 R] >>");
        obj!(3, "<< /Fields [5 0 R 6 0 R] >>");
        obj!(4, "<< /Type /Catalog /Pages 1 0 R /AcroForm 3 0 R >>");
        obj!(5, "<< /Type /Annot /Subtype /Widget /Rect [100 500 400 530] /P 2 0 R /FT /Tx /T (FullName) /V (John Doe) >>");
        obj!(6, "<< /Type /Annot /Subtype /Widget /Rect [100 450 130 480] /P 2 0 R /FT /Btn /T (AgreeTerms) /V /Off /AS /Off >>");
        let xref_offset = out.len();
        let obj_count = 6usize;
        out.extend_from_slice(b"xref\n");
        out.extend_from_slice(format!("0 {}\n", obj_count + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        out.extend_from_slice(xref_line.as_bytes());
        out.extend_from_slice(b"trailer\n");
        out.extend_from_slice(
            format!("<< /Size {} /Root 4 0 R >>\nstartxref\n{}\n%%EOF\n", obj_count + 1, xref_offset).as_bytes(),
        );
        out
    }

    /// ComboBox fixture (no writable form field in pdfium-render 0.8.37).
    fn build_combo_pdf_bytes() -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut xref_line = String::new();
        macro_rules! obj {
            ($n:expr, $body:expr) => {{
                xref_line.push_str(&format!("{:010} 00000 n \n", out.len()));
                out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", $n, $body).as_bytes());
            }};
        }
        out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
        obj!(1, "<< /Type /Pages /Kids [2 0 R] /Count 1 >>");
        obj!(2, "<< /Type /Page /Parent 1 0 R /MediaBox [0 0 792 612] /Annots [5 0 R] >>");
        obj!(3, "<< /Fields [5 0 R] >>");
        obj!(4, "<< /Type /Catalog /Pages 1 0 R /AcroForm 3 0 R >>");
        obj!(5, "<< /Type /Annot /Subtype /Widget /Rect [100 500 300 530] /P 2 0 R /FT /Ch /T (Country) /V (USA) /Opt [(USA) (UK) (JP)] >>");
        let xref_offset = out.len();
        let obj_count = 5usize;
        out.extend_from_slice(b"xref\n");
        out.extend_from_slice(format!("0 {}\n", obj_count + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        out.extend_from_slice(xref_line.as_bytes());
        out.extend_from_slice(b"trailer\n");
        out.extend_from_slice(
            format!("<< /Size {} /Root 4 0 R >>\nstartxref\n{}\n%%EOF\n", obj_count + 1, xref_offset).as_bytes(),
        );
        out
    }

    fn load<'a>(bytes: &'a [u8]) -> PdfDocument<'a> {
        // pdfium() returns &'static Pdfium, and PdfDocument<'a> borrows both
        // the library and the byte slice.
        let pdfium_inst = pdfium();
        pdfium_inst.load_pdf_from_byte_slice(bytes, None).unwrap()
    }

    /// find_target_widget: locates Text widget and returns its kind.
    #[test]
    fn find_target_text_widget() {
        let bytes = build_form_pdf_bytes();
        let doc = load(&bytes);
        let (page_idx, annot_idx, kind) =
            find_target_widget(&doc, "FullName").expect("FullName should be found");
        assert_eq!(page_idx, 0);
        assert_eq!(annot_idx, 0);
        assert_eq!(kind, FormFieldKind::Text);
    }

    /// find_target_widget: locates Checkbox widget and returns its kind.
    #[test]
    fn find_target_checkbox_widget() {
        let bytes = build_form_pdf_bytes();
        let doc = load(&bytes);
        let (_, _, kind) =
            find_target_widget(&doc, "AgreeTerms").expect("AgreeTerms should be found");
        assert_eq!(kind, FormFieldKind::Checkbox);
    }

    /// find_target_widget: returns None for unknown name.
    #[test]
    fn find_target_widget_missing_name() {
        let bytes = build_form_pdf_bytes();
        let doc = load(&bytes);
        assert!(find_target_widget(&doc, "NotPresent").is_none());
    }

    /// find_target_widget: ComboBox is recognised (kind = ListBox per pdfium).
    #[test]
    fn find_target_combo_widget() {
        let bytes = build_combo_pdf_bytes();
        let doc = load(&bytes);
        let (_, _, kind) =
            find_target_widget(&doc, "Country").expect("Country should be found");
        // pdfium maps /FT /Ch -> ListBox regardless of choice semantics
        assert_eq!(kind, FormFieldKind::ListBox);
    }
}
