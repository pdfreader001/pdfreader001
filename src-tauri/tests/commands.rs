//! M7 命令层集成测试：IPC 全链路（打开 → 编辑 → 保存 / 撤销重做 / 关闭）
//!
//! 与 tests/e2e.rs 的分工：
//! - e2e.rs 直连各模块的 `_logic` 纯函数（跨模块业务流程）。
//! - 本文件走 `tauri::test` 的 MockRuntime，调用**真实的 `#[tauri::command]`**，覆盖
//!   `generate_handler!` 注册、IPC body 反序列化、`State<AppState>` 注入、返回值序列化、
//!   错误 `{code, message}` 形状，即单元测试与 `_logic` 测试都碰不到的编排层
//!   （docId 分配、撤销/重做栈上限与清空语义、保存路径接管、关闭后句柄失效）。
//!
//! 为何不用 tauri-driver：WebDriver 只能驱动 WebView，驱动不了原生 Win32 文件对话框
//! （`plugin-dialog` 的 open/save），「打开 → 保存」在 UI 层无法自动化；命令层能覆盖同一
//! 编排，且零外部依赖（不需要 msedgedriver / tauri-driver）。
//!
//! 并发约束：同 e2e.rs —— pdfium 是进程级单例且本体非线程安全（`thread_safe` feature 只保证
//! 「同一时刻只有一个 Pdfium 实例」，不串行化单次 `FPDF_*` 调用），默认并行跑本文件会偶发
//! `PdfiumLibraryInternalError` 甚至堆损坏。故每个用例首行都取 `pdfium_serial()`。

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;

use pdfe_lib::document::AppState;

type MockApp = tauri::App<tauri::test::MockRuntime>;
type MockWindow = tauri::WebviewWindow<tauri::test::MockRuntime>;

/// 最小 mock 应用：只注册本文件用到的命令，状态与 lib.rs 一致（`AppState::default()`）。
fn mock_app() -> MockApp {
    mock_builder()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            pdfe_lib::document::open_document,
            pdfe_lib::document::close_document,
            pdfe_lib::document::get_metadata,
            pdfe_lib::document::save_document,
            pdfe_lib::document::undo_document,
            pdfe_lib::document::redo_document,
            pdfe_lib::document::can_undo,
            pdfe_lib::document::can_redo,
            pdfe_lib::document::undo_depth,
            pdfe_lib::document::redo_depth,
            pdfe_lib::edit::list_annotations,
            pdfe_lib::edit::add_annotation,
        ])
        .build(mock_context(noop_assets()))
        .expect("mock app 构建失败")
}

fn window(app: &MockApp) -> MockWindow {
    tauri::WebviewWindowBuilder::new(app, "main", Default::default())
        .build()
        .expect("mock webview 构建失败")
}

/// 发一次 IPC 请求（与前端 `invoke` 等价的载荷）。
fn invoke(window: &MockWindow, cmd: &str, args: Value) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.to_string(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "http://tauri.localhost".parse().expect("invoke url"),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|body| body.deserialize::<Value>().expect("命令返回值不是合法 JSON"))
}

/// 调用命令并断言成功。
fn call(window: &MockWindow, cmd: &str, args: Value) -> Value {
    invoke(window, cmd, args).unwrap_or_else(|e| panic!("命令 {cmd} 意外失败: {e}"))
}

/// 调用命令并断言失败，返回错误对象（`{code, message, [args]}`）。
fn call_err(window: &MockWindow, cmd: &str, args: Value) -> Value {
    match invoke(window, cmd, args) {
        Ok(v) => panic!("命令 {cmd} 本应失败，却返回 {v}"),
        Err(e) => e,
    }
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// 把 fixture 复制到独立临时目录（涉及写盘的用例绝不能碰 tests/fixtures 下的原件）。
fn temp_copy(label: &str, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfe_cmd_{label}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("创建临时目录失败");
    let dst = dir.join(name);
    fs::copy(fixture(name), &dst).expect("复制 fixture 失败");
    dst
}

/// 本测试二进制内的 pdfium 串行闸门（原因见文件头「并发约束」）。
fn pdfium_serial() -> std::sync::MutexGuard<'static, ()> {
    static PDFIUM_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    PDFIUM_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 打开文档并返回 docId。
fn open_doc(window: &MockWindow, path: &Path) -> u64 {
    let info = call(
        window,
        "open_document",
        json!({ "path": path.to_string_lossy(), "password": null }),
    );
    info["docId"].as_u64().expect("docId 是数字")
}

fn annotation_count(window: &MockWindow, doc_id: u64, page: u32) -> usize {
    call(
        window,
        "list_annotations",
        json!({ "docId": doc_id, "pageIndex": page }),
    )
    .as_array()
    .expect("注释列表是数组")
    .len()
}

fn add_highlight(window: &MockWindow, doc_id: u64, contents: &str) -> Value {
    call(
        window,
        "add_annotation",
        json!({
            "docId": doc_id,
            "pageIndex": 0,
            "opts": {
                "kind": "highlight",
                // 归一化区域（0–1，CSS 左上原点），与页面尺寸无关
                "region": { "left": 0.3, "top": 0.6, "width": 0.2, "height": 0.05 },
                "contents": contents,
                "color": "#FFFF00",
                "opacity": 0.5,
            },
        }),
    )
}

/// 场景 1：打开 → 编辑（加注释）→ 保存 → 关句柄 → 从磁盘重开验证持久化。
#[test]
fn cmd_open_annotate_save_roundtrip() {
    let _serial = pdfium_serial();
    let app = mock_app();
    let w = window(&app);

    let src = temp_copy("roundtrip", "sample.pdf");

    // 1) 打开
    let info = call(
        &w,
        "open_document",
        json!({ "path": src.to_string_lossy(), "password": null }),
    );
    let doc_id = info["docId"].as_u64().expect("docId 是数字");
    assert_eq!(info["fileName"], "sample.pdf");
    assert!(info["pageCount"].as_u64().unwrap() >= 1, "至少有 1 页");
    assert!(info["fileSizeBytes"].as_u64().unwrap() > 0, "字节数非零");

    // 2) 新打开的文档没有注释
    assert_eq!(annotation_count(&w, doc_id, 0), 0, "新文档无注释");

    // 3) 加一条高亮，列表可读回
    assert_eq!(add_highlight(&w, doc_id, "e2e note")["docId"], doc_id);
    let list = call(
        &w,
        "list_annotations",
        json!({ "docId": doc_id, "pageIndex": 0 }),
    );
    let arr = list.as_array().unwrap();
    assert_eq!(arr.len(), 1, "1 条注释");
    assert_eq!(arr[0]["kind"], "highlight");
    assert_eq!(arr[0]["contents"], "e2e note");
    // color_to_hex 统一输出小写（前端仅作 CSS 背景用，大小写无关）
    assert_eq!(arr[0]["color"], "#ffff00");
    assert_eq!(arr[0]["index"], 0);

    // 4) 保存到原路径（path 传 null → 用打开时的路径，等价前端 Ctrl+S）
    let saved = call(&w, "save_document", json!({ "docId": doc_id, "path": null }));
    assert_eq!(saved["fileName"], "sample.pdf");
    assert!(saved["fileSizeBytes"].as_u64().unwrap() > 0, "保存后非空");

    // 5) 关掉句柄，确保下面的读取只可能来自磁盘
    call(&w, "close_document", json!({ "docId": doc_id }));
    let reopened_id = open_doc(&w, &src);
    assert_ne!(reopened_id, doc_id, "docId 单调递增（旧句柄不复用）");
    let persisted = call(
        &w,
        "list_annotations",
        json!({ "docId": reopened_id, "pageIndex": 0 }),
    );
    let persisted = persisted.as_array().unwrap();
    assert_eq!(persisted.len(), 1, "重开后注释仍在（真正落盘）");
    assert_eq!(persisted[0]["contents"], "e2e note");
}

/// 场景 2：撤销/重做栈语义 —— 深度、can_* 标志、内容还原、新编辑后 redo 失效。
#[test]
fn cmd_undo_redo_stack_semantics() {
    let _serial = pdfium_serial();
    let app = mock_app();
    let w = window(&app);

    let src = temp_copy("undo_redo", "sample.pdf");
    let doc_id = open_doc(&w, &src);

    // 初始：没有可撤销/重做的内容
    assert_eq!(call(&w, "undo_depth", json!({ "docId": doc_id })), 0);
    assert_eq!(call(&w, "redo_depth", json!({ "docId": doc_id })), 0);
    assert_eq!(call(&w, "can_undo", json!({ "docId": doc_id })), false);
    assert_eq!(call(&w, "can_redo", json!({ "docId": doc_id })), false);
    // 无可撤销时应报 nothing_to_undo（非 not_found）
    let err = call_err(&w, "undo_document", json!({ "docId": doc_id }));
    assert_eq!(err["code"], "nothing_to_undo");

    // 两次编辑 → 撤销栈深度 2
    add_highlight(&w, doc_id, "note A");
    add_highlight(&w, doc_id, "note B");
    assert_eq!(annotation_count(&w, doc_id, 0), 2);
    assert_eq!(call(&w, "undo_depth", json!({ "docId": doc_id })), 2);
    assert_eq!(call(&w, "redo_depth", json!({ "docId": doc_id })), 0);
    assert_eq!(call(&w, "can_undo", json!({ "docId": doc_id })), true);
    assert_eq!(call(&w, "can_redo", json!({ "docId": doc_id })), false);

    // 撤销一步：内容回到 1 条注释，redo 栈出现 1 步
    call(&w, "undo_document", json!({ "docId": doc_id }));
    assert_eq!(annotation_count(&w, doc_id, 0), 1, "撤销后回到 1 条注释");
    assert_eq!(call(&w, "undo_depth", json!({ "docId": doc_id })), 1);
    assert_eq!(call(&w, "redo_depth", json!({ "docId": doc_id })), 1);
    assert_eq!(call(&w, "can_redo", json!({ "docId": doc_id })), true);

    // 重做：被撤销的编辑必须回到文档里
    call(&w, "redo_document", json!({ "docId": doc_id }));
    assert_eq!(annotation_count(&w, doc_id, 0), 2, "重做后回到 2 条注释");
    assert_eq!(call(&w, "undo_depth", json!({ "docId": doc_id })), 2);
    assert_eq!(call(&w, "redo_depth", json!({ "docId": doc_id })), 0);

    // 撤销后再做新编辑 → redo 栈作废（标准语义）
    call(&w, "undo_document", json!({ "docId": doc_id }));
    assert_eq!(call(&w, "redo_depth", json!({ "docId": doc_id })), 1);
    add_highlight(&w, doc_id, "note C");
    assert_eq!(call(&w, "redo_depth", json!({ "docId": doc_id })), 0, "新编辑清空 redo");
    assert_eq!(call(&w, "can_redo", json!({ "docId": doc_id })), false);
    assert_eq!(annotation_count(&w, doc_id, 0), 2, "note A + note C");
}

/// 场景 3：撤销栈上限（UNDO_LIMIT = 20），且清空后报 nothing_to_undo。
#[test]
fn cmd_undo_stack_respects_limit() {
    let _serial = pdfium_serial();
    let app = mock_app();
    let w = window(&app);

    let src = temp_copy("undo_limit", "sample.pdf");
    let doc_id = open_doc(&w, &src);

    for i in 0..25 {
        add_highlight(&w, doc_id, &format!("note {i}"));
    }
    assert_eq!(
        call(&w, "undo_depth", json!({ "docId": doc_id })).as_u64(),
        Some(20),
        "撤销栈封顶 20"
    );

    // 连续撤销 20 次都能成功，第 21 次报 nothing_to_undo
    for _ in 0..20 {
        let info = call(&w, "undo_document", json!({ "docId": doc_id }));
        assert_eq!(info["docId"].as_u64(), Some(doc_id));
    }
    assert_eq!(call(&w, "can_undo", json!({ "docId": doc_id })), false);
    let err = call_err(&w, "undo_document", json!({ "docId": doc_id }));
    assert_eq!(err["code"], "nothing_to_undo");
}

/// 场景 4：关闭文档后句柄失效，所有按 docId 的命令都报 not_found。
#[test]
fn cmd_close_document_releases_handle() {
    let _serial = pdfium_serial();
    let app = mock_app();
    let w = window(&app);

    let src = temp_copy("close", "sample.pdf");
    let doc_id = open_doc(&w, &src);
    add_highlight(&w, doc_id, "note");

    call(&w, "close_document", json!({ "docId": doc_id }));
    for cmd in ["get_metadata", "save_document", "undo_document", "list_annotations"] {
        let err = call_err(&w, cmd, json!({ "docId": doc_id, "pageIndex": 0 }));
        assert_eq!(err["code"], "not_found", "{cmd} 在关闭后应报 not_found");
    }
    // 只读探测不报错，直接返回 false/0
    assert_eq!(call(&w, "can_undo", json!({ "docId": doc_id })), false);
    assert_eq!(call(&w, "can_redo", json!({ "docId": doc_id })), false);
    assert_eq!(call(&w, "undo_depth", json!({ "docId": doc_id })), 0);

    // 重复关闭是幂等的
    call(&w, "close_document", json!({ "docId": doc_id }));
}

/// 场景 5：另存为 —— 新文件落盘、句柄切到新路径、原文件保持原样。
#[test]
fn cmd_save_to_new_path() {
    let _serial = pdfium_serial();
    let app = mock_app();
    let w = window(&app);

    let src = temp_copy("save_as", "sample.pdf");
    let dst = src.with_file_name("saved_copy.pdf");

    let doc_id = open_doc(&w, &src);
    add_highlight(&w, doc_id, "note");

    let saved = call(
        &w,
        "save_document",
        json!({ "docId": doc_id, "path": dst.to_string_lossy() }),
    );
    assert_eq!(saved["fileName"], "saved_copy.pdf", "句柄已切到新路径");
    assert!(dst.is_file(), "新文件已落盘");

    // 新文件带着注释
    let new_id = open_doc(&w, &dst);
    assert_eq!(annotation_count(&w, new_id, 0), 1, "新文件含注释");
    // 原文件未被改动
    let old_id = open_doc(&w, &src);
    assert_eq!(annotation_count(&w, old_id, 0), 0, "原文件保持原样");
}

/// 场景 6：边界 —— 打开不存在的路径走 io 错误码，且不留脏状态。
#[test]
fn cmd_open_missing_file_reports_io_error() {
    let _serial = pdfium_serial();
    let app = mock_app();
    let w = window(&app);

    let missing = std::env::temp_dir().join("pdfe_cmd_missing").join("nope.pdf");
    let err = call_err(
        &w,
        "open_document",
        json!({ "path": missing.to_string_lossy(), "password": null }),
    );
    assert_eq!(err["code"], "io");
    assert!(err["message"].is_string(), "错误带中文可读信息");

    // 失败不应占用 docId：随后正常打开拿到的是首个编号
    let src = temp_copy("missing_then_open", "sample.pdf");
    assert_eq!(open_doc(&w, &src), 1, "失败的开不影响 docId 分配");
}
