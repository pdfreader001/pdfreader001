//! 双击文字 pick_text_at_point 测试
use std::fs;
use std::path::PathBuf;

use pdfe_lib::render::pick_text_at_point_logic;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// sample.pdf 第 1 页第一个字符命中 "Hello"。
#[test]
fn pick_first_char_in_hello() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page0 = doc.pages().get(0).unwrap();
    let text = page0.text().unwrap();
    let page_chars = text.chars();
    let first = page_chars.get(0).unwrap();
    let b = first.tight_bounds().unwrap();
    let cx = (b.left().value + b.right().value) / 2.0;
    let cy = (b.bottom().value + b.top().value) / 2.0;

    let result = pick_text_at_point_logic(&bytes, 0, cx, cy).expect("调用失败");
    assert!(result.is_some(), "在第一个字符处应命中文字");
    let r = result.unwrap();
    assert_eq!(r.text, "Hello", "应选中 Hello 这个单词");
    assert!(r.right > r.left && r.top > r.bottom, "bounds 应有效");
    // 原字体样式：字号 > 0，颜色为 #rrggbb
    assert!(r.font_size > 0.0, "应读出原字号，实际 {}", r.font_size);
    assert_eq!(r.color.len(), 7, "颜色应为 #rrggbb，实际 {}", r.color);
    assert!(
        r.color.starts_with('#'),
        "颜色应以 # 开头，实际 {}",
        r.color
    );
}

/// 字体名应剥离子集前缀（`ABCDEF+SimSun` → `SimSun`）。
#[test]
fn strip_subset_prefix_removes_six_letter_tag() {
    assert_eq!(
        pdfe_lib::render::strip_subset_prefix("ABCDEF+SimSun"),
        "SimSun"
    );
    assert_eq!(
        pdfe_lib::render::strip_subset_prefix("Helvetica"),
        "Helvetica"
    );
    assert_eq!(pdfe_lib::render::strip_subset_prefix("abc+Def"), "abc+Def");
}

/// 点 (0, 0) 应不命中（页面外）。
#[test]
fn pick_outside_page_returns_none() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let result = pick_text_at_point_logic(&bytes, 0, 0.0, 0.0).unwrap();
    assert!(result.is_none(), "页面外的点应返回 None");
}

/// 页码越界应返回错误。
#[test]
fn pick_page_out_of_range() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let res = pick_text_at_point_logic(&bytes, 9999, 100.0, 700.0);
    assert!(res.is_err(), "页码越界应返回错误");
}

/// 损坏文件应返回错误。
#[test]
fn pick_corrupted_returns_error() {
    let garbage = b"\x00\x01\x02 not pdf";
    let res = pick_text_at_point_logic(garbage, 0, 100.0, 700.0);
    assert!(res.is_err(), "损坏文件应返回错误");
}

/// 第 2 页第一个字符命中 "Hello"。
#[test]
fn pick_second_page_text() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page1 = doc.pages().get(1).unwrap();
    let text = page1.text().unwrap();
    let page_chars = text.chars();
    let first = page_chars.get(0).unwrap();
    let b = first.tight_bounds().unwrap();
    let cx = (b.left().value + b.right().value) / 2.0;
    let cy = (b.bottom().value + b.top().value) / 2.0;

    let result = pick_text_at_point_logic(&bytes, 1, cx, cy).expect("调用失败");
    assert!(result.is_some());
    let r = result.unwrap();
    assert_eq!(r.text, "Hello");
}

/// 点空白字符处：只返回单字符。
#[test]
fn pick_space_returns_single_char() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page0 = doc.pages().get(0).unwrap();
    let text = page0.text().unwrap();
    let page_chars = text.chars();
    let mut space_idx = None;
    for (i, c) in page_chars.iter().enumerate() {
        if c.unicode_char() == Some(' ') {
            space_idx = Some(i);
            break;
        }
    }
    let i = space_idx.expect("应能找到空格字符");
    let sc = page_chars.get(i).unwrap();
    let b = sc.tight_bounds().unwrap();
    let cx = (b.left().value + b.right().value) / 2.0;
    let cy = (b.bottom().value + b.top().value) / 2.0;

    let result = pick_text_at_point_logic(&bytes, 0, cx, cy).expect("调用失败");
    assert!(result.is_some());
    let r = result.unwrap();
    assert_eq!(r.text, " ", "点击空白字符应只返回单个空白");
}
