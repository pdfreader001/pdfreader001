//! M7 性能基线测试
//!
//! 目标：
//! - 大文档（多页）场景下，关键路径（打开 / 翻页 / 渲染 / 搜索 / 文本提取 / 编辑 / 保存）的耗时回归保护
//! - 测试只验证「不超阈值」，不追求绝对速度，避免 CI 机器差异造成假阳性
//! - 带 `[perf]` 前缀的 eprintln 输出为实测画像，`cargo test --test perf -- --nocapture` 可查看
//!
//! 测试 PDF：
//! - tests/fixtures/sample.pdf（2 页，A4）
//! - tests/fixtures/large_100.pdf（100 页）/large_500.pdf（500 页）— 首次运行自动生成

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Once;
use std::time::{Duration, Instant};

use pdfe_lib::edit::{add_annotation_logic, AddAnnotationOpts, AnnotationKind, RegionSpec};
use pdfe_lib::render::{render_page_logic, search_page_text_logic};
use pdfium_render::prelude::*;

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

/// 本测试二进制内的 pdfium 串行闸门。
///
/// 背景（已实证）：pdfium 本体不是线程安全的。pdfium-render 的 `thread_safe`
/// feature 只在 `FPDF_InitLibrary` / `FPDF_DestroyLibrary` 之间持全局锁，也就是
/// 「同一时刻只允许存在一个 Pdfium 实例」，**并不会**串行化单个 FPDF_* 调用。
/// 而 `pdfe_lib::pdfium()` 返回进程内唯一的 `&'static Pdfium`，多线程同时使用它
/// 就会撞进 pdfium 的非线程安全实现。
///
/// 具体症状（默认多线程 `cargo test --test perf`）：
/// - 偶发 `PdfiumLibraryInternalError(Unknown)`
/// - 更危险的是静默损坏：从 296 KiB 的 500 页文档加载得到的 doc 几乎是空的，
///   重序列化后只写出 7 KiB 且测试「通过」。
///
/// 因此这里用一把测试内互斥锁把所有触碰 pdfium 的用例串行化，让结果可复现。
/// 注意：这只解决测试侧的可复现性，进程内并发调用 pdfium 的产品级风险仍存在。
fn pdfium_serial() -> std::sync::MutexGuard<'static, ()> {
    static PDFIUM_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    PDFIUM_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 渲染 1000 次同一页（模拟反复滚动进入视区）总耗时 < 5 秒。
#[test]
fn render_hot_loop_under_threshold() {
    let _serial = pdfium_serial();
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
    let _serial = pdfium_serial();
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
    let _serial = pdfium_serial();
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
    let _serial = pdfium_serial();
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
    let _serial = pdfium_serial();
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
    let _serial = pdfium_serial();
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
    let _serial = pdfium_serial();
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
    let _serial = pdfium_serial();
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
    let _serial = pdfium_serial();
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
    let _serial = pdfium_serial();
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
    let _serial = pdfium_serial();
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
    let _serial = pdfium_serial();
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
    let _serial = pdfium_serial();
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

// ==========================================================================
// M7 大文档（500 页）全链路画像：打开 → 翻页 → 编辑 → 保存 → 撤销栈内存
// ==========================================================================

/// 复刻 document.rs::inspect 的读取路径（打开文档时后端实际做的事：
/// 读盘 → pdfium 解析 → 遍历全部页面取尺寸）。
fn open_inspect(bytes: &[u8]) -> (u32, Vec<(f64, f64)>) {
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(bytes, None).expect("load");
    let pages = doc.pages();
    let count = pages.len() as u32;
    let mut sizes = Vec::with_capacity(count as usize);
    for i in 0..count {
        let idx = u16::try_from(i).expect("page index fits u16");
        let page = pages.get(idx).expect("get page");
        sizes.push((page.width().value as f64, page.height().value as f64));
    }
    (count, sizes)
}

/// 500 页文档：打开全链路 < 3s（读盘 + 解析 + 遍历 500 页尺寸）。
#[test]
fn large_500_open_full_path_under_3s() {
    let _serial = pdfium_serial();
    init_fixtures();
    let path = fixture("large_500.pdf");
    let start = Instant::now();
    let bytes = fs::read(&path).expect("read large_500");
    let (count, _sizes) = open_inspect(&bytes);
    let elapsed = start.elapsed();
    eprintln!(
        "[perf] 500 页打开：{:?}（{} 页，{} KiB）",
        elapsed,
        count,
        bytes.len() / 1024
    );
    assert_eq!(count, 500);
    assert!(
        elapsed < Duration::from_secs(3),
        "500 页文档打开 {:?} 超阈值 3s",
        elapsed
    );
}

/// 500 页文档：连续翻页 20 页，平均 < 200ms/页。
/// 每次迭代等价 render_page command：先 clone 一份文档 bytes，再解析并渲染单页。
#[test]
fn large_500_page_turn_avg_under_200ms() {
    let _serial = pdfium_serial();
    init_fixtures();
    let bytes = ensure_large_fixture(500);
    let pdfium = pdfium();
    let start = Instant::now();
    for i in 0..20u32 {
        let cloned = bytes.clone();
        let _ = render_page_logic(pdfium, &cloned, 200 + i, 1.0).unwrap();
    }
    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_secs_f64() / 20.0 * 1000.0;
    eprintln!(
        "[perf] 500 页翻页：20 页共 {:?}，平均 {:.1} ms/页",
        elapsed, avg_ms
    );
    assert!(
        avg_ms < 200.0,
        "500 页文档翻页平均 {:.1} ms 超阈值 200ms",
        avg_ms
    );
}

/// 500 页文档：单次编辑（添加注释 → 重序列化）< 1s。
#[test]
fn large_500_single_edit_under_1s() {
    let _serial = pdfium_serial();
    init_fixtures();
    let bytes = ensure_large_fixture(500);
    let region = RegionSpec {
        left: 100.0,
        top: 700.0,
        width: 200.0,
        height: 20.0,
    };
    let start = Instant::now();
    let out = add_annotation_logic(
        &bytes,
        250,
        &AddAnnotationOpts {
            kind: AnnotationKind::Highlight,
            color: "#FFFF00".into(),
            contents: "perf".into(),
            region,
            opacity: 0.5,
        },
    )
    .expect("add annotation on page 250");
    let elapsed = start.elapsed();
    eprintln!(
        "[perf] 500 页单次编辑：{:?}（{} KiB → {} KiB）",
        elapsed,
        bytes.len() / 1024,
        out.len() / 1024
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "500 页文档单次编辑 {:?} 超阈值 1s",
        elapsed
    );
}

/// 500 页文档：连续编辑 20 次（撤销栈满载）总耗时 < 20s。
#[test]
fn large_500_twenty_edits_under_20s() {
    let _serial = pdfium_serial();
    init_fixtures();
    let mut bytes = ensure_large_fixture(500);
    let region = RegionSpec {
        left: 100.0,
        top: 700.0,
        width: 200.0,
        height: 20.0,
    };
    let start = Instant::now();
    for i in 0..20u32 {
        bytes = add_annotation_logic(
            &bytes,
            i * 20,
            &AddAnnotationOpts {
                kind: AnnotationKind::StickyNote,
                color: "#FF8800".into(),
                contents: format!("perf {i}"),
                region: region.clone(),
                opacity: 0.5,
            },
        )
        .expect("add annotation");
    }
    let elapsed = start.elapsed();
    eprintln!("[perf] 500 页连续编辑 20 次：{:?}", elapsed);
    assert!(
        elapsed < Duration::from_secs(20),
        "500 页文档连续编辑 20 次 {:?} 超阈值 20s",
        elapsed
    );
}

/// 撤销栈内存画像：document.rs 每次修改前克隆整份文档字节，栈上限 20 份。
/// 这里只测克隆成本与常驻字节数（AppState 字段为 crate 私有，无法在集成测试里直接构造）。
#[test]
fn large_500_undo_snapshot_clone_cost_and_retention() {
    let _serial = pdfium_serial();
    init_fixtures();
    let bytes = ensure_large_fixture(500);
    let start = Instant::now();
    let mut snapshots: Vec<Vec<u8>> = Vec::with_capacity(20);
    for _ in 0..20 {
        snapshots.push(bytes.clone());
    }
    let elapsed = start.elapsed();
    let retained: usize = snapshots.iter().map(|s| s.len()).sum();
    eprintln!(
        "[perf] 500 页撤销栈：20 份快照共 {:.1} MiB，克隆耗时 {:?}",
        retained as f64 / (1024.0 * 1024.0),
        elapsed
    );
    // 每份快照与文档等长，20 份即 20 倍文件字节数（大文档内存占用的主要项）
    assert_eq!(retained, bytes.len() * 20);
    assert!(
        elapsed < Duration::from_millis(200),
        "20 份快照克隆 {:?} 超阈值 200ms",
        elapsed
    );
}

/// 500 页文档：保存（重序列化 + 原子写盘）< 2s。
#[test]
fn large_500_save_to_disk_under_2s() {
    let _serial = pdfium_serial();
    init_fixtures();
    let bytes = ensure_large_fixture(500);
    let pdfium = pdfium();
    let dir = std::env::temp_dir().join("pdfe_perf_save");
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("large_500_out.pdf");

    let start = Instant::now();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let new_bytes = doc.save_to_bytes().unwrap();
    drop(doc);
    pdfe_lib::document::atomic_write(&target, &new_bytes).unwrap();
    let elapsed = start.elapsed();

    let size = fs::metadata(&target).unwrap().len();
    // 回归保护：并发损坏时这里曾「成功」写出 7 KiB 的空文档（源文件 296 KiB）。
    let reopened = fs::read(&target).unwrap();
    fs::remove_file(&target).ok();
    let (reopened_pages, _) = open_inspect(&reopened);
    eprintln!(
        "[perf] 500 页保存：{:?}（写盘 {} KiB，重开 {} 页）",
        elapsed,
        size / 1024,
        reopened_pages
    );
    assert_eq!(
        reopened_pages,
        500,
        "保存后重开页数 {} ≠ 500，写盘 {} KiB（疑似文档被截断）",
        reopened_pages,
        size / 1024
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "500 页文档保存 {:?} 超阈值 2s",
        elapsed
    );
}
