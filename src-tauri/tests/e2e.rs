//! P7 端到端（E2E）测试
//!
//! 范围：跨模块的真实业务场景——打开 → 浏览/搜索 → 编辑（注释/重写） → 重开验证。
//! 策略：直接调用各模块的 `_logic` 纯函数（与 Tauri command 走同一逻辑路径），
//! 不启动 Tauri runtime 也不依赖 GUI，无需 tauri-driver。
//!
//! 与单模块测试的差异：
//! - 多步操作（每个步骤相当于前端的 IPC 调用）
//! - bytes 跨步骤流转（与前端的 `entry.bytes` 等价）
//! - 每步失败即中止，最终必须重开原始 fixture 验证持久化正确
//!
//! 并发约束：pdfium 本体不是线程安全的。pdfium-render 的 `thread_safe` feature 只在
//! `FPDF_InitLibrary` / `FPDF_DestroyLibrary` 之间持全局锁（即「同一时刻只允许存在一个
//! Pdfium 实例」），**并不**串行化单次 `FPDF_*` 调用；而 `pdfium()` 返回的是永不释放的
//! 进程级单例。默认并行跑本文件会偶发 `PdfiumLibraryInternalError`，甚至
//! `STATUS_HEAP_CORRUPTION`（0xC0000374）。故每个用例首行都取 `pdfium_serial()`。

use std::fs;
use std::path::PathBuf;

use pdfe_lib::document::pdfium;
use pdfe_lib::edit::{
    add_annotation_logic, list_annotations_logic, AddAnnotationOpts, AnnotationInfo,
    AnnotationKind, RegionSpec,
};
use pdfe_lib::edit_ext::{is_scanned_page_logic, rewrite_text_logic, PtRect, RewriteTextOpts};
use pdfe_lib::render::{
    get_page_text_logic, pick_text_at_point_logic, render_page_logic, search_page_text_logic,
};
use pdfe_lib::security::get_security_status_logic;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// 本测试二进制内的 pdfium 串行闸门（原因见文件头「并发约束」）。
fn pdfium_serial() -> std::sync::MutexGuard<'static, ()> {
    static PDFIUM_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    PDFIUM_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// E2E 场景 1：打开 → 浏览 → 搜索 → 退出
/// 验证：render + search + get_page_text 三模块协同。
#[test]
fn e2e_open_browse_search() {
    let _serial = pdfium_serial();
    let bytes = fs::read(fixture("sample.pdf")).expect("fixture missing");
    let pdfium_inst = pdfium();

    // 1) 打开 → 渲染第 0 页
    let _render = render_page_logic(pdfium_inst, &bytes, 0, 1.0).expect("page 0 renders");

    // 2) 浏览文本（第 0 页）
    let text0 = get_page_text_logic(&bytes, 0).expect("page 0 text");
    assert!(!text0.is_empty(), "page 0 has text");

    // 3) 全文搜索：用 page 0 第一个词作为查询
    let needle = text0
        .split_whitespace()
        .next()
        .unwrap_or("Hello")
        .to_string();
    let hits = search_page_text_logic(&bytes, 0, &needle, None).expect("search ok");
    assert!(!hits.hits.is_empty(), "search '{}' returns hits", needle);
    // 每个命中都有非空矩形
    for h in &hits.hits {
        assert!(h.right > h.left, "hit has non-empty rect");
    }
}

/// E2E 场景 2：打开 → 添加 3 个不同类型注释 → 重开 PDF 验证
/// 验证：注释数据写入持久化，重开后 list_annotations 能读回。
#[test]
fn e2e_annotate_reopen_persists() {
    let _serial = pdfium_serial();
    let initial = fs::read(fixture("sample.pdf")).expect("fixture missing");

    let pdfium_inst = pdfium();
    let doc = pdfium_inst.load_pdf_from_byte_slice(&initial, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    let pw = page.width().value as f32;
    let ph = page.height().value as f32;
    drop(doc);
    // PDF 坐标：左下原点，Y 向上
    let region = RegionSpec {
        left: pw * 0.3,
        top: ph * 0.6, // top 在 PDF 坐标里是上界
        width: pw * 0.2,
        height: ph * 0.05,
    };

    // 1) 打开 → 列表为空
    let before: Vec<AnnotationInfo> =
        list_annotations_logic(&initial, Some(0)).expect("list empty");
    assert!(before.is_empty(), "fresh doc has no annotations");

    // 2) 添加高亮
    let after_highlight = add_annotation_logic(
        &initial,
        0,
        &AddAnnotationOpts {
            kind: AnnotationKind::Highlight,
            color: "#FFFF00".into(),
            contents: "highlight note".into(),
            region: region.clone(),
            opacity: 0.5,
        },
    )
    .expect("add highlight");
    let after_highlight_list =
        list_annotations_logic(&after_highlight, Some(0)).expect("list after hi");
    assert_eq!(after_highlight_list.len(), 1, "1 highlight");

    // 3) 在新 bytes 上继续添加下划线
    let after_underline = add_annotation_logic(
        &after_highlight,
        0,
        &AddAnnotationOpts {
            kind: AnnotationKind::Underline,
            color: "#00FF00".into(),
            contents: "underline note".into(),
            region: region.clone(),
            opacity: 0.5,
        },
    )
    .expect("add underline");
    let after_underline_list =
        list_annotations_logic(&after_underline, Some(0)).expect("list after un");
    assert_eq!(after_underline_list.len(), 2, "2 annotations");

    // 4) 在新 bytes 上继续添加便签
    let after_sticky = add_annotation_logic(
        &after_underline,
        0,
        &AddAnnotationOpts {
            kind: AnnotationKind::StickyNote,
            color: "#FF8800".into(),
            contents: "sticky note".into(),
            region: region.clone(),
            opacity: 0.5,
        },
    )
    .expect("add sticky");
    let after_sticky_list =
        list_annotations_logic(&after_sticky, Some(0)).expect("list after st");
    assert_eq!(after_sticky_list.len(), 3, "3 annotations");

    // 5) 模拟"另存为磁盘"→"重新打开"：用磁盘写读模拟
    let tmp = std::env::temp_dir().join("pdfe_e2e_annot.pdf");
    fs::write(&tmp, &after_sticky).expect("write tmp");
    let reopened = fs::read(&tmp).expect("read tmp");
    fs::remove_file(&tmp).ok();

    // 6) 重开后列表验证
    let reopened_list = list_annotations_logic(&reopened, Some(0)).expect("list reopened");
    assert_eq!(
        reopened_list.len(),
        3,
        "annotations persist after save+reopen"
    );

    // 7) 全文档列表（None）也应包含这 3 个
    let all_list = list_annotations_logic(&reopened, None).expect("list all");
    assert_eq!(all_list.len(), 3, "all-pages list has 3");
}

/// E2E 场景 3：打开 → 文本重写 → 重开验证
/// 验证：rewrite_text 改变 PDF 文本层，重开后 get_page_text 能读到新文本。
#[test]
fn e2e_rewrite_text_persists() {
    let _serial = pdfium_serial();
    let initial = fs::read(fixture("sample.pdf")).expect("fixture missing");

    // 1) 重写前：读第 0 页文本
    let before = get_page_text_logic(&initial, 0).expect("text before");
    assert!(!before.is_empty());

    // 2) 重写：覆盖 page 0 一个区域
    let pdfium_inst = pdfium();
    let doc = pdfium_inst.load_pdf_from_byte_slice(&initial, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    let pw = page.width().value as f32;
    let ph = page.height().value as f32;
    drop(doc);

    let region = PtRect {
        left: pw * 0.1,
        bottom: ph * 0.4,
        right: pw * 0.9,
        top: ph * 0.6,
    };
    let (new_bytes, _approximated) = rewrite_text_logic(
        pdfium_inst,
        &initial,
        0,
        region,
        &RewriteTextOpts {
            new_text: "E2E-REWRITTEN-2024".into(),
            font_size: 12.0,
            color: "#000000".into(),
            font_name: None,
        },
    )
    .expect("rewrite text");

    // 3) 重写后立即重读：新 bytes 立即能读出新文本
    let after = get_page_text_logic(&new_bytes, 0).expect("text after");
    assert!(
        after.contains("E2E-REWRITTEN-2024"),
        "rewritten text visible in same bytes, got: {:?}",
        after
    );

    // 4) 模拟"另存为磁盘"→"重新打开"
    let tmp = std::env::temp_dir().join("pdfe_e2e_rewrite.pdf");
    fs::write(&tmp, &new_bytes).expect("write tmp");
    let reopened = fs::read(&tmp).expect("read tmp");
    fs::remove_file(&tmp).ok();

    let after_reopen = get_page_text_logic(&reopened, 0).expect("text after reopen");
    assert!(
        after_reopen.contains("E2E-REWRITTEN-2024"),
        "rewritten text visible after save+reopen, got: {:?}",
        after_reopen
    );
}

/// E2E 场景 4：打开 → pick_text_at_point → get_page_text 交叉验证
/// 验证：pick_text 给出的字符文本确实存在于 get_page_text 全文中。
#[test]
fn e2e_pick_text_consistent_with_full_text() {
    let _serial = pdfium_serial();
    let bytes = fs::read(fixture("sample.pdf")).expect("fixture missing");
    let pdfium_inst = pdfium();

    // 1) 拿全文
    let full = get_page_text_logic(&bytes, 0).expect("full text");
    assert!(!full.is_empty());

    // 2) 渲染第 0 页
    let _render = render_page_logic(pdfium_inst, &bytes, 0, 1.0).expect("render");

    // 3) 用 page 0 第一个字符的实际 tight_bounds 中心
    let doc = pdfium_inst.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let page0 = doc.pages().get(0).unwrap();
    let text_obj = page0.text().unwrap();
    let page_chars = text_obj.chars();
    let first = page_chars.get(0).expect("page has at least one char");
    let b = first.tight_bounds().expect("first char has bounds");
    let cx = (b.left().value + b.right().value) / 2.0;
    let cy = (b.bottom().value + b.top().value) / 2.0;
    drop(doc);

    // 4) 在第一个字符中心点 pick
    let pick = pick_text_at_point_logic(&bytes, 0, cx, cy)
        .expect("pick call ok")
        .expect("first char center should hit text");
    assert!(!pick.text.is_empty(), "picked text not empty");

    // 5) pick 的文本应该是 full text 的一个子串
    assert!(
        full.contains(&pick.text),
        "pick text {:?} must be substring of full text {:?}",
        pick.text,
        full
    );
}

/// E2E 场景 5：打开 → 文档诊断全模块组合
/// 验证：security + scanned + annotations + metadata 一次性全部能读。
#[test]
fn e2e_diagnose_all_modules() {
    let _serial = pdfium_serial();
    let bytes = fs::read(fixture("sample.pdf")).expect("fixture missing");
    let pdfium_inst = pdfium();

    // 1) security 模块
    let doc = pdfium_inst.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let sec = get_security_status_logic(&doc);
    assert_eq!(sec.handler_revision, "Unprotected");
    drop(doc);

    // 2) scanned 模块（第 0 页）
    let doc2 = pdfium_inst.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let scanned = is_scanned_page_logic(&doc2, 0);
    assert!(!scanned, "sample.pdf page 0 is not scanned");
    drop(doc2);

    // 3) annotations 模块
    let annots = list_annotations_logic(&bytes, None).expect("list annots");
    assert!(annots.is_empty(), "sample.pdf has no annotations");

    // 4) metadata: 通过 get_page_text 验证至少 page 0 可读
    let _t0 = get_page_text_logic(&bytes, 0).expect("page 0");
}

/// E2E 场景 6：search_page_text + get_page_text 命中位置正确性
/// 验证：search 返回的非空命中数量 ≤ 文本出现次数。
#[test]
fn e2e_search_hit_count_matches_text_count() {
    let _serial = pdfium_serial();
    let bytes = fs::read(fixture("sample.pdf")).expect("fixture missing");
    let full = get_page_text_logic(&bytes, 0).expect("full text");

    // 找全文中至少 3 字符的单词
    let probe_words: Vec<&str> = full
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .collect();
    if probe_words.is_empty() {
        return; // 没有可用单词，跳过
    }
    let needle = probe_words[0];
    let result = search_page_text_logic(&bytes, 0, needle, None).expect("search");

    // 文本中 needle 出现次数（按字符直接 count）
    let text_count = full.matches(needle).count();
    // 命中矩形数应 ≤ 文本出现次数（可能合并相邻命中）
    assert!(
        result.hits.len() <= text_count.max(1),
        "search hits {} should be <= text occurrences {}",
        result.hits.len(),
        text_count
    );
    assert!(
        !result.hits.is_empty() || text_count == 0,
        "if text has '{}', search should find at least one hit",
        needle
    );
}

/// E2E 场景 7：bytes 引用语义——修改不污染源
/// 验证：每次写操作返回新 bytes，源文件 bytes 保持不变。
#[test]
fn e2e_bytes_immutability_across_steps() {
    let _serial = pdfium_serial();
    let initial = fs::read(fixture("sample.pdf")).expect("fixture missing");
    let initial_len = initial.len();
    let initial_hash: u32 = initial
        .iter()
        .fold(0u32, |h, &b| h.wrapping_mul(31).wrapping_add(b as u32));

    let pdfium_inst = pdfium();
    let doc = pdfium_inst.load_pdf_from_byte_slice(&initial, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    let pw = page.width().value as f32;
    let ph = page.height().value as f32;
    drop(doc);

    let region = RegionSpec {
        left: pw * 0.2,
        top: ph * 0.7,
        width: pw * 0.3,
        height: ph * 0.1,
    };
    let _ = add_annotation_logic(
        &initial,
        0,
        &AddAnnotationOpts {
            kind: AnnotationKind::Highlight,
            color: "#FF0000".into(),
            contents: "test".into(),
            region,
            opacity: 0.5,
        },
    )
    .expect("add1");

    // 初始 bytes 仍可用，hash 不变
    let current_hash: u32 = initial
        .iter()
        .fold(0u32, |h, &b| h.wrapping_mul(31).wrapping_add(b as u32));
    assert_eq!(initial_hash, current_hash, "source bytes unchanged");
    assert_eq!(initial.len(), initial_len, "length unchanged");

    // 重读磁盘文件应与初始一致
    let still_initial = fs::read(fixture("sample.pdf")).unwrap();
    assert_eq!(still_initial.len(), initial_len, "disk file unchanged");
}

/// E2E 场景 8：安全加密文档诊断（用有密码的 fixture）
/// 验证：get_security_status 对加密文档返回非 "Unprotected"。
/// 
/// 跳过条件：项目 fixtures/ 中无加密 PDF。此处使用 sample.pdf 作 baseline，
/// 场景的真正价值在 security.rs 单测中已覆盖。
#[test]
fn e2e_diagnose_unprotected_baseline() {
    let _serial = pdfium_serial();
    let bytes = fs::read(fixture("sample.pdf")).expect("fixture missing");
    let pdfium_inst = pdfium();
    let doc = pdfium_inst.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let sec = get_security_status_logic(&doc);

    // baseline：sample.pdf 未加密 → Unprotected + 全权限
    assert_eq!(sec.handler_revision, "Unprotected");
    assert!(sec.can_modify_document, "baseline allows modify");
    assert!(
        sec.can_extract_text_and_graphics,
        "baseline allows extract"
    );
}
