//! M7 大文档（500 页）内存画像测试
//!
//! 与 `tests/perf.rs` 的耗时断言互补：这里量的是进程工作集（RSS）随各阶段的增长，
//! 回答「打开 500 页文档、渲染、以及撤销栈常驻各占多少内存」。
//!
//! 为什么单独一个文件：cargo 里每个 `tests/*.rs` 是独立的测试二进制、彼此串行执行，
//! 而**同一个二进制内的用例默认并行**。本文件只放一个 `#[test]`，独占一个进程，
//! 因此工作集测量不会被同进程内的其他用例污染，也不需要 `--test-threads=1`。
//!
//! 运行：`cargo test --test perf_memory -- --nocapture`
//!
//! 阈值刻意宽松：目的是给出画像与粗粒度回归保护，避免不同机器上的假阳性。

#![cfg(windows)]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use pdfium_render::prelude::*;

/// 通过 kernel32 读取当前进程的工作集，避免为测试引入额外依赖。
mod win_mem {
    use std::ffi::c_void;

    /// 对应 Win32 的 `PROCESS_MEMORY_COUNTERS`。
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    impl ProcessMemoryCounters {
        fn zeroed() -> Self {
            ProcessMemoryCounters {
                cb: std::mem::size_of::<Self>() as u32,
                page_fault_count: 0,
                peak_working_set_size: 0,
                working_set_size: 0,
                quota_peak_paged_pool_usage: 0,
                quota_paged_pool_usage: 0,
                quota_peak_non_paged_pool_usage: 0,
                quota_non_paged_pool_usage: 0,
                pagefile_usage: 0,
                peak_pagefile_usage: 0,
            }
        }
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    /// 当前进程工作集字节数；查询失败返回 0。
    pub fn working_set_bytes() -> usize {
        let mut counters = ProcessMemoryCounters::zeroed();
        let ok =
            unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
        if ok != 0 {
            counters.working_set_size
        } else {
            0
        }
    }
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// 与 perf.rs 共用同一份 fixture；缺失时现场生成，保证本测试可独立运行。
fn ensure_large_fixture(page_count: u32) -> Vec<u8> {
    let path = fixture(&format!("large_{page_count}.pdf"));
    if let Ok(bytes) = fs::read(&path) {
        return bytes;
    }
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.create_new_pdf().expect("create pdf");
    let token = doc.fonts_mut().helvetica();
    for i in 0..page_count {
        doc.pages_mut()
            .create_page_at_index(PdfPagePaperSize::a4(), i as u16)
            .expect("create page");
    }
    {
        let pages = doc.pages_mut();
        for i in 0..page_count {
            let mut page = pages.get(i as u16).expect("get page");
            let text = format!("Page {} of large fixture", i + 1);
            let _ = page
                .objects_mut()
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
    let bytes = doc.save_to_bytes().expect("save bytes");
    fs::File::create(&path)
        .expect("create fixture")
        .write_all(&bytes)
        .expect("write fixture");
    bytes
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// 500 页文档的内存画像：基线 → 解析 → 渲染峰值 → 撤销栈常驻。
#[test]
fn large_500_memory_profile() {
    const PAGES: u32 = 500;

    let bytes = ensure_large_fixture(PAGES);
    let file_kib = bytes.len() / 1024;

    // 预热：把 pdfium.dll 加载、库初始化、字体缓存等一次性开销移出测量区间。
    {
        let pdfium = pdfe_lib::pdfium();
        let warm = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
        let _ = warm.pages().get(0).unwrap();
    }
    let baseline = win_mem::working_set_bytes();

    // 阶段一：解析整份文档并遍历全部页面取尺寸（document::inspect 的路径），
    // 文档保持存活以观察「解析结果常驻」的开销。
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let pages = doc.pages();
    let count = pages.len() as u32;
    assert_eq!(count, PAGES, "fixture 页数应为 {PAGES}");
    for i in 0..count {
        let page = pages.get(u16::try_from(i).unwrap()).unwrap();
        let _ = (page.width().value, page.height().value);
    }
    let after_parse = win_mem::working_set_bytes();

    // 阶段二：渲染 5 页（模拟画布进入视区）。渲染位图是峰值的主要来源：
    // A4@1x 约 794×1123×4 B ≈ 3.4 MiB/页。
    let mut render_peak = after_parse;
    for i in 0..5u32 {
        let page = pages.get(250 + u16::try_from(i).unwrap()).unwrap();
        let width = page.width().value as i32;
        let height = page.height().value as i32;
        let bitmap = page.render(width, height, None).unwrap();
        render_peak = render_peak.max(win_mem::working_set_bytes());
        drop(bitmap);
    }
    let after_render = win_mem::working_set_bytes();

    drop(doc);
    let after_drop = win_mem::working_set_bytes();

    // 阶段三：撤销栈常驻。document.rs 每次修改前克隆整份字节，栈上限 20 份，
    // 因此大文档下这是常驻内存的主要项。
    let clone_start = Instant::now();
    let mut snapshots: Vec<Vec<u8>> = Vec::with_capacity(20);
    for _ in 0..20 {
        snapshots.push(bytes.clone());
    }
    let clone_elapsed = clone_start.elapsed();
    let after_snapshots = win_mem::working_set_bytes();
    let retained: usize = snapshots.iter().map(|s| s.len()).sum();

    eprintln!("[perf-mem] 500 页内存画像（文件 {} KiB）", file_kib);
    eprintln!(
        "[perf-mem]   基线（预热后）      : {:.1} MiB",
        mib(baseline)
    );
    eprintln!(
        "[perf-mem]   解析后（文档存活）  : {:.1} MiB（+{:.1} MiB）",
        mib(after_parse),
        mib(after_parse.saturating_sub(baseline))
    );
    eprintln!(
        "[perf-mem]   渲染 5 页峰值       : {:.1} MiB（+{:.1} MiB）",
        mib(render_peak),
        mib(render_peak.saturating_sub(after_parse))
    );
    eprintln!(
        "[perf-mem]   渲染后（位图释放）  : {:.1} MiB（+{:.1} MiB）",
        mib(after_render),
        mib(after_render.saturating_sub(after_parse))
    );
    eprintln!(
        "[perf-mem]   释放文档后          : {:.1} MiB（+{:.1} MiB）",
        mib(after_drop),
        mib(after_drop.saturating_sub(baseline))
    );
    eprintln!(
        "[perf-mem]   20 份撤销快照后     : {:.1} MiB（+{:.1} MiB，克隆耗时 {:?}）",
        mib(after_snapshots),
        mib(after_snapshots.saturating_sub(after_drop)),
        clone_elapsed
    );
    eprintln!(
        "[perf-mem]   撤销栈逻辑字节      : {:.1} MiB（20 × {} KiB）",
        mib(retained),
        file_kib
    );

    // 宽松回归保护：撤销栈每份与文档等长（大文档常驻内存的主要来源）。
    assert_eq!(retained, bytes.len() * 20);
    // 快照确实驻留，且量级与逻辑字节数同阶（允许分配器与 RSS 统计的偏差）。
    assert!(
        after_snapshots.saturating_sub(after_drop) >= retained / 2,
        "20 份快照后工作集仅增长 {:.1} MiB，低于逻辑字节数的一半（{:.1} MiB）",
        mib(after_snapshots.saturating_sub(after_drop)),
        mib(retained / 2)
    );
}
