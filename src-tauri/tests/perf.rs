//! M7 性能基线测试
//!
//! 目标：
//! - 大文档（多页）场景下，关键路径（渲染 / 搜索 / 文本提取）的耗时回归保护
//! - 测试只验证「不超阈值」，不追求绝对速度，避免 CI 机器差异造成假阳性
//!
//! 测试 PDF：tests/fixtures/sample.pdf（2 页，A4）。

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pdfium_render::prelude::*;
use pdfe_lib::render::{render_page_logic, search_page_text_logic};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn pdfium<'a>() -> &'a Pdfium {
    pdfe_lib::pdfium()
}

/// 渲染 1000 次同一页（模拟反复滚动进入视区）总耗时 < 5 秒。
#[test]
fn render_hot_loop_under_threshold() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    let scale = 1.0;

    let start = Instant::now();
    for _ in 0..1000 {
        let _ = render_page_logic(pdfium, &bytes, 0, scale).unwrap();
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "1000 次渲染耗时 {} 超阈值 5 秒",
        elapsed.as_secs_f64()
    );
}

/// 单页渲染在合理时间内完成（< 200ms）。
#[test]
fn single_render_under_200ms() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfium();
    let start = Instant::now();
    let _ = render_page_logic(pdfium, &bytes, 0, 1.0).unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(200),
        "单页渲染耗时 {:?} 超阈值 200ms",
        elapsed
    );
}

/// 1000 次搜索"the"总耗时 < 5 秒（模拟用户频繁 F3）。
#[test]
fn search_hot_loop_under_threshold() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let start = Instant::now();
    for _ in 0..1000 {
        let _ = search_page_text_logic(&bytes, 0, "the", Some(50)).unwrap();
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "1000 次搜索耗时 {} 超阈值 5 秒",
        elapsed.as_secs_f64()
    );
}

/// 单页搜索耗时 < 100ms。
#[test]
fn single_search_under_100ms() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let start = Instant::now();
    let _ = search_page_text_logic(&bytes, 0, "the", Some(50)).unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(100),
        "单页搜索耗时 {:?} 超阈值 100ms",
        elapsed
    );
}

/// 文档重新序列化（明文副本导出等价行为）耗时 < 100ms。
#[test]
fn save_reserialize_under_100ms() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let start = Instant::now();
    let _ = doc.save_to_bytes().unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(100),
        "save_to_bytes 耗时 {:?} 超阈值 100ms",
        elapsed
    );
}

/// 大 DPI 渲染（如导出图片 600 DPI）也能在合理时间内完成。
#[test]
fn high_dpi_render_under_2s() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfium();
    let start = Instant::now();
    // 600/72 ≈ 8.3x 缩放
    let _ = render_page_logic(pdfium, &bytes, 0, 8.3).unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "600 DPI 渲染耗时 {:?} 超阈值 2 秒",
        elapsed
    );
}