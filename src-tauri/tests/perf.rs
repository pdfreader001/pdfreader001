//! M7 性能基线测试
//!
//! 目标：
//! - 大文档（多页）场景下，关键路径（渲染 / 搜索 / 文本提取）的耗时回归保护
//! - 测试只验证「不超阈值」，不追求绝对速度，避免 CI 机器差异造成假阳性
//!
//! 测试 PDF：
//! - tests/fixtures/sample.pdf（2 页，A4）
//! - tests/fixtures/large_100.pdf（100 页）/large_500.pdf（500 页）— 首次运行自动生成

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Once;
use std::time::{Duration, Instant};

use pdfium_render::prelude::*;
use pdfe_lib::render::{render_page_logic, search_page_text_logic};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// 生成大文档 fixture（首次运行慢，之后直接复用）。
fn build_large_pdf_bytes(page_count: u32) -> Vec<u8> {
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.create_new_pdf().expect("create pdf");
    let token = doc.fonts_mut().helvetica();
    for i in 0..page_count {
        let size = PdfPagePaperSize::a4();
        doc.pages_mut()
            .create_page_at_index(size, i as u16)
            .expect("create page");
    }
    {
        let pages = doc.pages_mut();
        for i in 0..page_count {
            let mut page = pages.get(i as u16).expect("get page");
            let objects = page.objects_mut();
            let text = format!("Page {} of large fixture", i + 1);
            let _ = objects
                .create_text_object(
                    PdfPoints::new(72.0),
                    PdfPoints::new(720.0),
                    &text,
                    token,
                    PdfPoints::new(18.0),
                )
                .expect("create text");
        }
    }
    doc.save_to_bytes().expect("save bytes")
}

fn ensure_large_fixture(page_count: u32) -> Vec<u8> {
    let path = fixture(&format!("large_{page_count}.pdf"));
    if path.exists() {
        return fs::read(&path).expect("read fixture");
    }
    eprintln!(
        "首次运行：生成 large_{page_count}.pdf（约 {} 页，可能 30s+）",
        page_count
    );
    let bytes = build_large_pdf_bytes(page_count);
    fs::File::create(&path)
        .expect("create fixture")
        .write_all(&bytes)
        .expect("write fixture");
    bytes
}

/// 一次性初始化所有大文档 fixture（避免每个测试重复生成）。
static INIT_FIXTURES: Once = Once::new();
fn init_fixtures() {
    INIT_FIXTURES.call_once(|| {
        ensure_large_fixture(100);
        ensure_large_fixture(500);
    });
}

fn pdfium<'a>() -> &'a Pdfium {
    pdfe_lib::pdfium()
}

/// 渲染 1000 次同一页（模拟反复滚动进入视区）总耗时 < 5 秒。
#[test]
fn render_hot_loop_under_threshold() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfium();
    let _doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
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

// ==========================================================================
// 大文档场景（500 页）：关键路径在内存/IO 上的扩展性
// ==========================================================================

/// 100 页文档：单页渲染 < 500ms。
#[test]
fn large_100_single_render_under_500ms() {
    init_fixtures();
    let bytes = ensure_large_fixture(100);
    let pdfium = pdfium();
    let start = Instant::now();
    let _ = render_page_logic(pdfium, &bytes, 50, 1.0).unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "100 页文档单页渲染 {:?} 超阈值 500ms",
        elapsed
    );
}

/// 500 页文档：单页渲染 < 1s（pdfium 加载会慢，但单页渲染仍要可控）。
#[test]
fn large_500_single_render_under_1s() {
    init_fixtures();
    let bytes = ensure_large_fixture(500);
    let pdfium = pdfium();
    let start = Instant::now();
    let _ = render_page_logic(pdfium, &bytes, 250, 1.0).unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(1),
        "500 页文档单页渲染 {:?} 超阈值 1s",
        elapsed
    );
}

/// 500 页文档：连续渲染 50 页 < 30s（模拟快速滚动浏览大文档）。
#[test]
fn large_500_scroll_50_pages_under_30s() {
    init_fixtures();
    let bytes = ensure_large_fixture(500);
    let pdfium = pdfium();
    let start = Instant::now();
    for i in 0..50 {
        let _ = render_page_logic(pdfium, &bytes, i * 10, 1.0).unwrap();
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "500 页文档 50 页渲染 {:?} 超阈值 30s",
        elapsed
    );
}

/// 100 页文档：单页搜索 < 500ms（覆盖"Page of large fixture"）。
#[test]
fn large_100_single_search_under_500ms() {
    init_fixtures();
    let bytes = ensure_large_fixture(100);
    let start = Instant::now();
    let _ = search_page_text_logic(&bytes, 50, "Page", Some(20)).unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "100 页文档单页搜索 {:?} 超阈值 500ms",
        elapsed
    );
}

/// 100 页文档：单页文本提取 < 500ms。
#[test]
fn large_100_get_page_text_under_500ms() {
    init_fixtures();
    let bytes = ensure_large_fixture(100);
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    assert_eq!(doc.pages().len(), 100);
    let start = Instant::now();
    let _ = pdfe_lib::render::get_page_text_logic(&bytes, 50).unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "100 页文档文本提取 {:?} 超阈值 500ms",
        elapsed
    );
}

/// 500 页文档：单页文本提取 < 1s。
#[test]
fn large_500_get_page_text_under_1s() {
    init_fixtures();
    let bytes = ensure_large_fixture(500);
    let start = Instant::now();
    let _ = pdfe_lib::render::get_page_text_logic(&bytes, 250).unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(1),
        "500 页文档文本提取 {:?} 超阈值 1s",
        elapsed
    );
}

/// 100 页文档：save_to_bytes 重序列化 < 2s（明文副本导出等价行为）。
#[test]
fn large_100_save_under_2s() {
    init_fixtures();
    let bytes = ensure_large_fixture(100);
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let start = Instant::now();
    let _ = doc.save_to_bytes().unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "100 页文档重序列化 {:?} 超阈值 2s",
        elapsed
    );
}