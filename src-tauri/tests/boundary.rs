//! 边界条件测试：覆盖常见异常输入与状态边界。
//!
//! 重点：损坏文件、空查询、越界页码、非空文件 vs 0 页、空字符串、错误类型传播。
use std::fs;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// 损坏文件（完全无效的字节）应返回 Damaged 或 Io 错误。
#[test]
fn open_corrupted_bytes() {
    let pdfium = pdfe_lib::pdfium();
    let garbage = b"\x00\x01\x02\x03not a pdf at all\xff\xff\xff";
    let res = pdfium.load_pdf_from_byte_slice(garbage, None);
    assert!(res.is_err(), "损坏字节应被拒绝");
}

/// 空字节应返回错误。
#[test]
fn open_empty_bytes() {
    let pdfium = pdfe_lib::pdfium();
    let res = pdfium.load_pdf_from_byte_slice(&[], None);
    assert!(res.is_err(), "空文件应被拒绝");
}

/// PDF 头有效但内容残缺：截断的 PDF 也应被拒绝。
#[test]
fn open_truncated_pdf() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let truncated = &bytes[..bytes.len() / 2];
    let pdfium = pdfe_lib::pdfium();
    let res = pdfium.load_pdf_from_byte_slice(truncated, None);
    assert!(res.is_err(), "截断 PDF 应被拒绝");
}

/// 页码越界访问应返回错误。
#[test]
fn get_page_out_of_range() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let total = doc.pages().len();
    // 尝试访问越界页
    let res = doc.pages().get(total as u16);
    assert!(res.is_err(), "访问 {} 页索引 {} 应越界", total, total);
}

/// 有效 PDF 的空文本搜索返回空结果。
#[test]
fn search_empty_query_via_logic() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let result = pdfe_lib::render::search_page_text_logic(&bytes, 0, "", Some(10)).unwrap();
    assert_eq!(result.hits.len(), 0);
    assert_eq!(result.page_index, 0);
}

/// 搜索仅含空白字符的查询。
#[test]
fn search_whitespace_query() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let result = pdfe_lib::render::search_page_text_logic(&bytes, 0, "   ", Some(10)).unwrap();
    // search_page_text_logic 内部 query.trim()，空白查询应等同空查询
    assert_eq!(result.hits.len(), 0);
}

/// max_hits = 0 应返回空结果。
#[test]
fn search_max_hits_zero() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let result =
        pdfe_lib::render::search_page_text_logic(&bytes, 0, "Hello", Some(0)).unwrap();
    assert_eq!(result.hits.len(), 0);
}

/// 无效页码的搜索返回错误。
#[test]
fn search_invalid_page_index() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    // sample.pdf 有 2 页，索引 100 越界
    let res = pdfe_lib::render::search_page_text_logic(&bytes, 100, "Hello", Some(10));
    assert!(res.is_err(), "页码 100 应返回错误");
}

/// 单页文本提取在页码 1（第二页）应返回非空字符串。
#[test]
fn get_page_text_second_page() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let text = pdfe_lib::render::get_page_text_logic(&bytes, 1).unwrap();
    assert!(text.contains("Page 2"), "第二页应包含 Page 2 文字");
}

/// 损坏文件的文本提取应返回错误。
#[test]
fn get_page_text_corrupted() {
    let garbage = b"\x00\x01\x02 not pdf";
    let res = pdfe_lib::render::get_page_text_logic(garbage, 0);
    assert!(res.is_err(), "损坏文件文本提取应失败");
}

/// 保存 - 重新打开往返：bytes 大小可能变化但页数必须一致。
#[test]
fn save_then_load_roundtrip_preserves_pagecount() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let original_count = doc.pages().len();
    let saved = doc.save_to_bytes().unwrap();

    let doc2 = pdfium.load_pdf_from_byte_slice(&saved, None).unwrap();
    assert_eq!(doc2.pages().len(), original_count, "往返后页数必须一致");
}

/// 错误码序列化：所有错误变体的 code() 必须返回非空字符串。
#[test]
fn all_error_codes_nonempty() {
    use pdfe_lib::error::AppError;
    let cases: Vec<AppError> = vec![
        AppError::Password,
        AppError::Damaged,
        AppError::NotFound,
        AppError::PageOutOfRange,
        AppError::Security,
        AppError::NothingToUndo,
        AppError::NothingToRedo,
        AppError::NoSavePath,
        AppError::NoPagesToDelete,
        AppError::CannotDeleteAllPages,
        AppError::NoPagesForWatermark,
        AppError::TextEmpty,
        AppError::NoPagesToExport,
        AppError::InvalidRect,
        AppError::NoCandidates,
        AppError::SearchFailed,
        AppError::NoImagesProvided,
        AppError::PdfWriteFailed,
    ];
    for e in &cases {
        let code = e.code();
        assert!(!code.is_empty(), "错误变体 code 不能为空");
        // 序列化应成功
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"code\""), "序列化 JSON 应包含 code 字段");
        assert!(json.contains(&code), "JSON 应包含 code 字符串");
    }
}

/// 空字节数组的页面渲染 → 错误。
#[test]
fn render_empty_bytes_fails() {
    let pdfium = pdfe_lib::pdfium();
    let res = pdfium.load_pdf_from_byte_slice(&[], None);
    assert!(res.is_err());
}

/// 加载大尺寸 PDF（MediaBox 非常大）应能正常打开（不崩溃）。
#[test]
fn open_large_page_pdf_via_inmemory() {
    use pdfium_render::prelude::{PdfPagePaperSize, PdfPoints};
    let pdfium = pdfe_lib::pdfium();
    let mut doc = pdfium.create_new_pdf().unwrap();
    // 创建 100x100 英寸的页面（超大但合法）
    {
        let pages = doc.pages_mut();
        let _ = pages
            .create_page_at_index(
                PdfPagePaperSize::Custom(PdfPoints::new(7200.0), PdfPoints::new(7200.0)),
                0,
            )
            .unwrap();
    }
    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    assert_eq!(doc2.pages().len(), 1);
    let p = doc2.pages().get(0).unwrap();
    assert!((p.width().value - 7200.0).abs() < 0.01);
}

/// PageRangeSpec 反序列化：基本字段都应正常解析。
#[test]
fn page_range_spec_deserialize_ok() {
    use pdfe_lib::convert::PageRangeSpec;
    let json = r#"{"pages": [0, 1, 2], "dpi": 150.0, "format": "png"}"#;
    let spec: PageRangeSpec = serde_json::from_str(json).expect("反序列化失败");
    assert_eq!(spec.pages, vec![0, 1, 2]);
    assert!((spec.dpi - 150.0).abs() < 0.01);
    assert_eq!(spec.format, "png");
}

/// PageRangeSpec 边界值：空 pages 仍可反序列化（业务层应拦截）。
#[test]
fn page_range_spec_empty_pages() {
    use pdfe_lib::convert::PageRangeSpec;
    let json = r#"{"pages": [], "dpi": 72.0, "format": "jpeg"}"#;
    let spec: PageRangeSpec = serde_json::from_str(json).expect("反序列化失败");
    assert!(spec.pages.is_empty());
}

/// PageRangeSpec 极端 DPI：0、负数、超大值都能反序列化（业务层有 clamp）。
#[test]
fn page_range_spec_extreme_dpi() {
    use pdfe_lib::convert::PageRangeSpec;
    for dpi_str in ["0.0", "-100.0", "100000.0"] {
        let json = format!(r#"{{"pages": [0], "dpi": {dpi_str}, "format": "png"}}"#);
        let spec: PageRangeSpec = serde_json::from_str(&json).expect("反序列化失败");
        // 仅验证反序列化不崩；具体 clamp 在命令实现里
        let _ = spec.dpi;
    }
}

/// ImageToPdfOpts 反序列化。
#[test]
fn image_to_pdf_opts_deserialize() {
    use pdfe_lib::convert::ImageToPdfOpts;
    let json = r#"{"imagePaths": ["a.png", "b.jpg"], "pageSize": "a4", "layout": "fit"}"#;
    let opts: ImageToPdfOpts = serde_json::from_str(json).expect("反序列化失败");
    assert_eq!(opts.image_paths.len(), 2);
    assert_eq!(opts.page_size, "a4");
    assert_eq!(opts.layout, "fit");
}

/// ImageToPdfOpts 空图片路径仍能反序列化（业务层 NoImagesProvided 拦截）。
#[test]
fn image_to_pdf_opts_empty_paths() {
    use pdfe_lib::convert::ImageToPdfOpts;
    let json = r#"{"imagePaths": [], "pageSize": "fit", "layout": "fit"}"#;
    let opts: ImageToPdfOpts = serde_json::from_str(json).expect("反序列化失败");
    assert!(opts.image_paths.is_empty());
}

// ==========================================================================
// watermark_remove：Rect 几何 & 边界
// ==========================================================================

/// 两个矩形相交判定。
#[test]
fn rects_intersect_basic() {
    use pdfe_lib::watermark_remove::{rects_intersect, Rect};
    let a = Rect { left: 0.0, bottom: 0.0, right: 100.0, top: 100.0 };
    let b = Rect { left: 50.0, bottom: 50.0, right: 150.0, top: 150.0 };
    assert!(rects_intersect(&a, &b), "重叠应判定为相交");

    let c = Rect { left: 200.0, bottom: 200.0, right: 300.0, top: 300.0 };
    assert!(!rects_intersect(&a, &c), "分离矩形不应相交");
}

/// 边界接触：恰好边对齐的矩形应判为不相交（半开区间）。
#[test]
fn rects_intersect_edge_touching() {
    use pdfe_lib::watermark_remove::{rects_intersect, Rect};
    let a = Rect { left: 0.0, bottom: 0.0, right: 100.0, top: 100.0 };
    // 右侧恰好贴在 a 的右边
    let b = Rect { left: 100.0, bottom: 0.0, right: 200.0, top: 100.0 };
    assert!(!rects_intersect(&a, &b), "边对齐应判为不相交");
}

/// 完全包含的矩形应相交。
#[test]
fn rects_intersect_contained() {
    use pdfe_lib::watermark_remove::{rects_intersect, Rect};
    let outer = Rect { left: 0.0, bottom: 0.0, right: 100.0, top: 100.0 };
    let inner = Rect { left: 10.0, bottom: 10.0, right: 90.0, top: 90.0 };
    assert!(rects_intersect(&outer, &inner));
    assert!(rects_intersect(&inner, &outer));
}

/// 退化矩形：零宽矩形被另一个矩形严格包含时视为相交。
#[test]
fn rects_intersect_degenerate() {
    use pdfe_lib::watermark_remove::{rects_intersect, Rect};
    // 零宽线段被矩形严格包含
    let line = Rect { left: 50.0, bottom: 0.0, right: 50.0, top: 100.0 };
    let area = Rect { left: 40.0, bottom: 0.0, right: 60.0, top: 100.0 };
    assert!(
        rects_intersect(&line, &area),
        "零宽矩形被包含时视为相交"
    );

    // 完全分离的零宽矩形
    let line_c = Rect { left: 200.0, bottom: 0.0, right: 200.0, top: 100.0 };
    assert!(
        !rects_intersect(&line, &line_c),
        "分离的零宽矩形不应相交"
    );
}

/// Rect 反序列化：缺字段应失败。
#[test]
fn rect_deserialize_missing_field() {
    use pdfe_lib::watermark_remove::Rect;
    let json = r#"{"left": 0, "bottom": 0, "right": 100}"#; // 缺 top
    let res: Result<Rect, _> = serde_json::from_str(json);
    assert!(res.is_err(), "缺少必填字段应反序列化失败");
}

/// Rect 反序列化：字段全在。
#[test]
fn rect_deserialize_ok() {
    use pdfe_lib::watermark_remove::Rect;
    let json = r#"{"left": 10, "bottom": 20, "right": 110, "top": 220}"#;
    let r: Rect = serde_json::from_str(json).expect("反序列化失败");
    assert_eq!(r.left, 10.0);
    assert_eq!(r.bottom, 20.0);
    assert_eq!(r.right, 110.0);
    assert_eq!(r.top, 220.0);
}

// ==========================================================================
// edit_ext：扫描版检测纯函数
// ==========================================================================

/// is_scanned_page_logic 页码越界：返回 true（按扫描版处理）。
#[test]
fn is_scanned_page_out_of_range() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let res = pdfe_lib::edit_ext::is_scanned_page_logic(&doc, 100);
    assert!(res, "页码越界按扫描版处理");
}

/// is_scanned_page_logic 正常 PDF 含文本 → false（非扫描版）。
#[test]
fn is_scanned_page_normal_pdf() {
    let bytes = fs::read(fixture("sample.pdf")).unwrap();
    let pdfium = pdfe_lib::pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let res = pdfe_lib::edit_ext::is_scanned_page_logic(&doc, 0);
    assert!(!res, "含文本的页面应判为非扫描版");
}

// ==========================================================================
// 错误处理 review：错误码字典一致性 + args 完整性 + 序列化结构
// ==========================================================================

/// 所有 AppError 变体的 code() 都不重复（防止抄错或漏配）。
#[test]
fn all_error_codes_are_unique() {
    use pdfe_lib::error::AppError;
    let cases: Vec<AppError> = vec![
        AppError::Password,
        AppError::Damaged,
        AppError::NotFound,
        AppError::PageOutOfRange,
        AppError::Security,
        AppError::Io(std::io::Error::other("test")),
        AppError::Internal("test".into()),
        AppError::NothingToUndo,
        AppError::NothingToRedo,
        AppError::NoSavePath,
        AppError::NoPagesToDelete,
        AppError::CannotDeleteAllPages,
        AppError::NoPagesToDuplicate,
        AppError::NoPagesToMove,
        AppError::NoPagesToExtract,
        AppError::InvalidPageRange { range: "1-".into() },
        AppError::NeedAtLeastOneFile,
        AppError::MergeResultEmpty,
        AppError::PagesPerFileZero,
        AppError::NothingToSplit,
        AppError::WatermarkTextEmpty,
        AppError::InvalidImageSize,
        AppError::NoPagesForWatermark,
        AppError::NoChineseFont,
        AppError::AnnotationOutOfRange,
        AppError::TextEmpty,
        AppError::NoPagesToExport,
        AppError::DpiOutOfRange { dpi: 1000, min: 36, max: 600 },
        AppError::ImageConstructFailed,
        AppError::NoImagesProvided,
        AppError::ImageReadFailed { path: "/a/b".into() },
        AppError::PdfWriteFailed,
        AppError::InvalidRect,
        AppError::NoCandidates,
        AppError::SearchFailed,
        AppError::ToolNotFound { tool: "soffice".into() },
        AppError::UnsupportedFormat { format: "xls".into() },
        AppError::SourceNotFound { path: "/a".into() },
        AppError::ToolStartFailed { tool: "soffice".into(), detail: "err".into() },
        AppError::ToolFailed { tool: "soffice".into(), code: 1 },
        AppError::CannotDetermineSourceName,
        AppError::NoPdfGenerated,
    ];
    let mut seen = std::collections::HashSet::new();
    for e in &cases {
        let code = e.code();
        assert!(seen.insert(code), "错误码重复：{}", code);
    }
}

/// 所有带 args 的 AppError 变体的 args() 都包含声明的字段。
#[test]
fn error_args_contain_declared_fields() {
    use pdfe_lib::error::AppError;
    let cases: Vec<(AppError, Vec<&str>)> = vec![
        (AppError::Io(std::io::Error::other("x")), vec!["detail"]),
        (AppError::Internal("msg".into()), vec!["detail"]),
        (AppError::InvalidPageRange { range: "x".into() }, vec!["range"]),
        (
            AppError::DpiOutOfRange { dpi: 1000, min: 36, max: 600 },
            vec!["dpi", "min", "max"],
        ),
        (AppError::ToolNotFound { tool: "x".into() }, vec!["tool"]),
        (AppError::UnsupportedFormat { format: "x".into() }, vec!["format"]),
        (AppError::SourceNotFound { path: "x".into() }, vec!["path"]),
        (
            AppError::ToolStartFailed { tool: "x".into(), detail: "y".into() },
            vec!["tool", "detail"],
        ),
        (
            AppError::ToolFailed { tool: "x".into(), code: 1 },
            vec!["tool", "code"],
        ),
        (AppError::ImageReadFailed { path: "x".into() }, vec!["path"]),
    ];
    for (e, expected_keys) in cases {
        let args = e.args();
        for k in expected_keys {
            assert!(args.contains_key(k), "{} 缺少 args 字段 {}", e.code(), k);
        }
    }
}

/// 带 args 的错误 JSON 序列化必须包含 args 字段（前端依赖）。
#[test]
fn error_with_args_serializes_args() {
    use pdfe_lib::error::AppError;
    use serde_json::Value;
    let cases = vec![
        AppError::Io(std::io::Error::other("disk full")),
        AppError::InvalidPageRange { range: "1-2-x".into() },
        AppError::DpiOutOfRange { dpi: 1000, min: 36, max: 600 },
        AppError::ToolNotFound { tool: "soffice".into() },
    ];
    for e in cases {
        let json = serde_json::to_string(&e).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("args").is_some(), "{} 应有 args 字段", e.code());
        assert!(v.get("args").unwrap().is_object(), "args 必须是对象");
    }
}

/// 无 args 的错误 JSON 序列化必须省略 args 字段。
#[test]
fn error_without_args_omits_args() {
    use pdfe_lib::error::AppError;
    use serde_json::Value;
    let cases = vec![
        AppError::Password,
        AppError::Damaged,
        AppError::NotFound,
        AppError::PageOutOfRange,
        AppError::TextEmpty,
        AppError::InvalidRect,
        AppError::NoImagesProvided,
        AppError::PdfWriteFailed,
    ];
    for e in cases {
        let json = serde_json::to_string(&e).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("args").is_none(), "{} 不应有 args 字段", e.code());
        assert!(v.get("code").is_some(), "code 字段必须有");
        assert!(v.get("message").is_some(), "message 字段必须有");
    }
}

/// JSON 序列化的 code 字段值应与 code() 返回值一致。
#[test]
fn error_json_code_matches_method() {
    use pdfe_lib::error::AppError;
    use serde_json::Value;
    let e = AppError::DpiOutOfRange { dpi: 1000, min: 36, max: 600 };
    let json = serde_json::to_string(&e).unwrap();
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["code"].as_str().unwrap(), e.code());
    assert_eq!(v["args"]["dpi"].as_str().unwrap(), "1000");
    assert_eq!(v["args"]["min"].as_str().unwrap(), "36");
    assert_eq!(v["args"]["max"].as_str().unwrap(), "600");
}

/// Io 错误的 message 应可读（包含原始错误信息）。
#[test]
fn error_io_message_is_readable() {
    use pdfe_lib::error::AppError;
    let e = AppError::Io(std::io::Error::other("disk full"));
    let msg = e.to_string();
    assert!(msg.contains("disk full"), "Io 错误消息应包含 detail: {}", msg);
}

/// Internal 错误的 message 应可读。
#[test]
fn error_internal_message_is_readable() {
    use pdfe_lib::error::AppError;
    let e = AppError::Internal("detailed problem description".into());
    let msg = e.to_string();
    assert!(msg.contains("detailed problem description"), "Internal 错误消息应包含原文: {}", msg);
}
