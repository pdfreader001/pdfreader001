//! 搜索高亮测试：验证 search_page_text 返回正确的命中矩形。
use std::fs;
use std::path::PathBuf;

use pdfe_lib::render::{search_page_text_logic, PageSearchResult};
use pdfium_render::prelude::*;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn pdfium<'a>() -> &'a Pdfium {
    pdfe_lib::pdfium()
}

/// 样例 PDF 第 1 页搜索 "PDF" 应返回若干命中，且矩形有效。
#[test]
fn search_sample_pdf_first_page() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let result: PageSearchResult = search_page_text_logic(&bytes, 0, "PDF", Some(20)).unwrap();

    assert_eq!(result.page_index, 0);
    assert!(!result.hits.is_empty(), "应该至少找到 1 个 PDF 命中");
    println!("找到 {} 个命中", result.hits.len());

    for (i, h) in result.hits.iter().enumerate() {
        assert!(h.right > h.left, "命中 {} 宽度应 > 0", i);
        assert!(h.top > h.bottom, "命中 {} 高度应 > 0", i);
        println!(
            "  命中 {}: left={:.1} bottom={:.1} right={:.1} top={:.1} w={:.1} h={:.1}",
            i,
            h.left,
            h.bottom,
            h.right,
            h.top,
            h.right - h.left,
            h.top - h.bottom
        );
    }
}

/// 搜索不存在的字符串应返回空结果。
#[test]
fn search_nonexistent_returns_empty() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let result =
        search_page_text_logic(&bytes, 0, "ThisStringDefinitelyDoesNotExist12345", Some(10))
            .unwrap();
    assert_eq!(result.hits.len(), 0);
}

/// 空 query 返回空结果。
#[test]
fn search_empty_query_returns_empty() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let result = search_page_text_logic(&bytes, 0, "", Some(10)).unwrap();
    assert_eq!(result.hits.len(), 0);
}

/// 搜索中文关键词（如有）。
#[test]
fn search_chinese_if_present() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    // 先确认第 1 页有什么文字
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page0 = doc.pages().get(0).unwrap();
    let text = page0.text().unwrap().all();
    println!("page0 text: {:?}", &text[..text.len().min(200)]);

    // 搜索 "the"（常见英文词）
    let result = search_page_text_logic(&bytes, 0, "the", Some(20)).unwrap();
    if !result.hits.is_empty() {
        println!("'the' 命中数: {}", result.hits.len());
        // 验证第一个命中的坐标在页面范围内
        let h = &result.hits[0];
        assert!(h.left >= 0.0 && h.left < 700.0);
        assert!(h.bottom >= 0.0 && h.bottom < 800.0);
    }
}

/// 页码越界应返回错误。
#[test]
fn search_page_out_of_range() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let result = search_page_text_logic(&bytes, 9999, "PDF", Some(10));
    assert!(result.is_err(), "页码越界应返回错误");
}

/// max_hits = 1 时最多返回 1 个命中。
#[test]
fn search_max_hits_one() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    // 先确认默认能找到多个 "the"
    let all = search_page_text_logic(&bytes, 0, "the", Some(100)).unwrap();
    if all.hits.len() <= 1 {
        println!("'the' 只有 {} 个命中，跳过 max_hits 测试", all.hits.len());
        return;
    }
    let limited = search_page_text_logic(&bytes, 0, "the", Some(1)).unwrap();
    assert_eq!(limited.hits.len(), 1, "max_hits=1 应只返回 1 个");
}

/// 所有命中矩形都在页面范围内。
#[test]
fn search_hits_within_page_bounds() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page0 = doc.pages().get(0).unwrap();
    let page_w = page0.width().value;
    let page_h = page0.height().value;

    let result = search_page_text_logic(&bytes, 0, "e", Some(50)).unwrap();
    for (i, h) in result.hits.iter().enumerate() {
        assert!(h.left >= 0.0, "命中 {} left 左边界", i);
        assert!(h.right <= page_w, "命中 {} right 超右边界", i);
        assert!(h.bottom >= 0.0, "命中 {} bottom 超下边界", i);
        assert!(h.top <= page_h, "命中 {} top 超上边界", i);
        assert!(h.right > h.left, "命中 {} 宽度应为正", i);
        assert!(h.top > h.bottom, "命中 {} 高度应为正", i);
    }
    println!(
        "验证 {} 个命中都在页面范围内 ({:.1} x {:.1})",
        result.hits.len(),
        page_w,
        page_h
    );
}

/// 大小写不区分大小写搜索（pdfium 默认不区分大小写）。
#[test]
fn search_case_insensitive() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let upper = search_page_text_logic(&bytes, 0, "PDF", Some(20)).unwrap();
    let lower = search_page_text_logic(&bytes, 0, "pdf", Some(20)).unwrap();
    // 两种大小写应返回相同数量的命中
    assert_eq!(upper.hits.len(), lower.hits.len(), "搜索应不区分大小写");
}
