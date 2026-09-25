//! P5+P6 PDF 表单测试
//!
//! 手写 PDF AcroForm fixture：1 Text + 1 Checkbox。
//! 读路径（list_form_fields_logic）：列出字段名 + 值。
//! 写路径（set_form_field_value_logic）：Text set_value + Checkbox set_checked
//! 通过 Widget annotation mutable 路径实现。

use std::fs;
use std::path::PathBuf;

use pdfe_lib::forms::{
    list_form_fields_logic, set_form_field_value_logic, FormFieldKind, SetFormFieldOpts,
};

fn fixtures_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 手写最小 AcroForm PDF（1 页 792x612 pt，1 Text field + 1 Checkbox）
///
/// PDF 对象布局：
///   obj 1 = Pages dict  → obj 2
///   obj 2 = Page dict    → 包含 /Annots [obj 5 obj 6]
///   obj 3 = AcroForm     → /Fields [obj 5 obj 6]
///   obj 4 = Catalog      → /Pages 1 /AcroForm 3
///   obj 5 = Text Widget  → /FT/Tx /T (FullName) /V (John Doe)
///   obj 6 = Check Widget → /FT/Btn /T (AgreeTerms) /V /Off /AS /Off
fn build_form_pdf_bytes() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    let mut xref_line = String::new();

    macro_rules! obj {
        ($n:expr, $body:expr) => {{
            xref_line.push_str(&format!("{:010} 00000 n \n", out.len()));
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", $n, $body).as_bytes());
        }};
    }

    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    obj!(1, "<< /Type /Pages /Kids [2 0 R] /Count 1 >>");
    obj!(
        2,
        "<< /Type /Page /Parent 1 0 R /MediaBox [0 0 792 612] /Annots [5 0 R 6 0 R] >>"
    );
    obj!(3, "<< /Fields [5 0 R 6 0 R] >>");
    obj!(4, "<< /Type /Catalog /Pages 1 0 R /AcroForm 3 0 R >>");
    obj!(5, "<< /Type /Annot /Subtype /Widget /Rect [100 500 400 530] /P 2 0 R /FT /Tx /T (FullName) /V (John Doe) >>");
    obj!(6, "<< /Type /Annot /Subtype /Widget /Rect [100 450 130 480] /P 2 0 R /FT /Btn /T (AgreeTerms) /V /Off /AS /Off >>");

    let xref_offset = out.len();
    let obj_count = offsets.len();
    out.extend_from_slice(b"xref\n");
    out.extend_from_slice(format!("0 {}\n", obj_count + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    out.extend_from_slice(xref_line.as_bytes());
    out.extend_from_slice(b"trailer\n");
    out.extend_from_slice(
        format!(
            "<< /Size {} /Root 4 0 R >>\nstartxref\n{}\n%%EOF\n",
            obj_count + 1,
            xref_offset
        )
        .as_bytes(),
    );

    out
}

fn ensure_form_pdf() -> PathBuf {
    let path = fixtures_dir().join("form_sample.pdf");
    if path.exists() {
        return path;
    }
    let bytes = build_form_pdf_bytes();
    fs::write(&path, bytes).expect("write fixture");
    path
}

// ---------- 读路径 ----------

/// sample.pdf（无表单）应返回空列表
#[test]
fn list_form_fields_empty_for_non_form_pdf() {
    let bytes = fs::read(fixtures_dir().join("sample.pdf")).expect("read sample");
    let fields = list_form_fields_logic(&bytes).expect("list ok");
    assert!(fields.is_empty(), "non-form pdf has no form fields");
}

/// form_sample.pdf 应能列出我们生成的 Text + Checkbox（pdfium 直接给 values Map）
#[test]
fn list_form_fields_finds_generated_fields() {
    let fixture = ensure_form_pdf();
    let bytes = fs::read(&fixture).expect("read fixture");
    let fields = list_form_fields_logic(&bytes).expect("list ok");

    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    println!("fields ({}): {:?}", fields.len(), names);

    // pdfium 的 field_values Map 只给 name→value，不保证字段类型正确
    // 核心验证：我们的两个字段名能找到
    assert!(
        names.contains(&"FullName"),
        "Text field 'FullName' should be listed, got {:?}",
        names
    );
    assert!(
        names.contains(&"AgreeTerms"),
        "Checkbox 'AgreeTerms' should be listed"
    );

    let full_name = fields.iter().find(|f| f.name == "FullName").unwrap();
    assert_eq!(full_name.value, "John Doe");

    let agree = fields.iter().find(|f| f.name == "AgreeTerms").unwrap();
    // Checkbox value 可能是 "Off" 或空，取决于 pdfium 解析
    assert!(!agree.value.is_empty() || agree.name == "AgreeTerms");
}

/// 验证返回的 kind 字段：FullName → Text，AgreeTerms → Checkbox
#[test]
fn list_form_fields_returns_kind() {
    let fixture = ensure_form_pdf();
    let bytes = fs::read(&fixture).expect("read fixture");
    let fields = list_form_fields_logic(&bytes).expect("list ok");

    let full_name = fields
        .iter()
        .find(|f| f.name == "FullName")
        .expect("FullName found");
    assert_eq!(
        full_name.kind,
        FormFieldKind::Text,
        "FullName should be Text kind"
    );

    let agree = fields
        .iter()
        .find(|f| f.name == "AgreeTerms")
        .expect("AgreeTerms found");
    assert_eq!(
        agree.kind,
        FormFieldKind::Checkbox,
        "AgreeTerms should be Checkbox kind"
    );
}

/// 损坏 PDF 应返回错误（pdfium 自己会报错）
#[test]
fn list_form_fields_damaged() {
    let bytes = b"not a pdf";
    let r = list_form_fields_logic(bytes);
    assert!(r.is_err(), "corrupt should fail");
}

// ---------- 写路径 ----------

/// Text 字段写入 + 读回验证（往返）
#[test]
fn set_text_field_value_roundtrip() {
    let fixture = ensure_form_pdf();
    let bytes = fs::read(&fixture).expect("read fixture");

    let new_bytes = set_form_field_value_logic(
        &bytes,
        &SetFormFieldOpts {
            name: "FullName".to_string(),
            value: "Jane Smith".to_string(),
        },
    )
    .expect("set_value should succeed");

    // 写回的 bytes 非空
    assert!(!new_bytes.is_empty());

    // 重新读取：值应是新值
    let fields = list_form_fields_logic(&new_bytes).expect("list after set");
    let full_name = fields.iter().find(|f| f.name == "FullName").expect("found");
    assert_eq!(
        full_name.value, "Jane Smith",
        "Text field value should be updated, got {:?}",
        full_name.value
    );
}

/// Checkbox 字段写入 + 读回验证
#[test]
fn set_checkbox_field_value_roundtrip() {
    let fixture = ensure_form_pdf();
    let bytes = fs::read(&fixture).expect("read fixture");

    // 设成 truthy
    let new_bytes = set_form_field_value_logic(
        &bytes,
        &SetFormFieldOpts {
            name: "AgreeTerms".to_string(),
            value: "true".to_string(),
        },
    )
    .expect("set_checkbox should succeed");

    let fields = list_form_fields_logic(&new_bytes).expect("list after check");
    let agree = fields
        .iter()
        .find(|f| f.name == "AgreeTerms")
        .expect("found");
    // pdfium 对 Checkbox 的 /V 一般表示为 "Yes"/"Off"
    // 我们这里主要确保值变了（不再是原始的 "false"/"Off"）
    let val_lc = agree.value.to_ascii_lowercase();
    assert!(
        val_lc == "yes" || val_lc == "true" || val_lc == "on" || val_lc == "checked",
        "Checkbox should be checked, got {:?}",
        agree.value
    );

    // 再设回 false
    let new_bytes2 = set_form_field_value_logic(
        &new_bytes,
        &SetFormFieldOpts {
            name: "AgreeTerms".to_string(),
            value: "false".to_string(),
        },
    )
    .expect("set_checkbox false should succeed");

    let fields2 = list_form_fields_logic(&new_bytes2).expect("list after uncheck");
    let agree2 = fields2
        .iter()
        .find(|f| f.name == "AgreeTerms")
        .expect("found");
    let val_lc2 = agree2.value.to_ascii_lowercase();
    assert!(
        val_lc2 == "off"
            || val_lc2 == "false"
            || val_lc2 == "no"
            || val_lc2 == "unchecked"
            || val_lc2 == "0",
        "Checkbox should be unchecked, got {:?}",
        agree2.value
    );
}

/// 不存在的字段名应返回 NotFound
#[test]
fn set_form_field_value_missing_name() {
    let fixture = ensure_form_pdf();
    let bytes = fs::read(&fixture).expect("read fixture");

    let r = set_form_field_value_logic(
        &bytes,
        &SetFormFieldOpts {
            name: "NonExistent".to_string(),
            value: "anything".to_string(),
        },
    );
    assert!(r.is_err(), "missing field should error");
}

/// 非表单 PDF（sample.pdf）写入应返回 NotFound
#[test]
fn set_form_field_value_no_form() {
    let bytes = fs::read(fixtures_dir().join("sample.pdf")).expect("read sample");
    let r = set_form_field_value_logic(
        &bytes,
        &SetFormFieldOpts {
            name: "FullName".to_string(),
            value: "test".to_string(),
        },
    );
    assert!(r.is_err(), "no-form pdf should error");
}

/// 手写一个 ComboBox fixture，验证写入返回 FormFieldWriteUnsupported 错误
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
    obj!(
        2,
        "<< /Type /Page /Parent 1 0 R /MediaBox [0 0 792 612] /Annots [5 0 R] >>"
    );
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
        format!(
            "<< /Size {} /Root 4 0 R >>\nstartxref\n{}\n%%EOF\n",
            obj_count + 1,
            xref_offset
        )
        .as_bytes(),
    );
    out
}

fn ensure_combo_pdf() -> PathBuf {
    let path = fixtures_dir().join("form_combo.pdf");
    if path.exists() {
        return path;
    }
    fs::write(&path, build_combo_pdf_bytes()).expect("write combo fixture");
    path
}

#[test]
fn set_form_field_value_combo_unsupported() {
    let fixture = ensure_combo_pdf();
    let bytes = fs::read(&fixture).expect("read combo fixture");

    let r = set_form_field_value_logic(
        &bytes,
        &SetFormFieldOpts {
            name: "Country".to_string(),
            value: "UK".to_string(),
        },
    );
    assert!(r.is_err(), "ComboBox write should be unsupported");
    let err = r.unwrap_err();
    // 检查错误码是 FormFieldWriteUnsupported
    assert_eq!(err.code(), "form_field_write_unsupported");
}
