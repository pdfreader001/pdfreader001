use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use pdfium_render::prelude::*;
use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};

/// 撤销快照栈条数上限。
const UNDO_LIMIT: usize = 20;

/// 单个文档的撤销 / 重做快照栈内存预算（字节，两侧各自独立计算）。
///
/// 只按条数封顶约束不住内存：20 个 50 MB 的快照就是 1 GB。超出预算时从栈底淘汰
/// 最旧的快照 —— 快照只在栈尾写入与弹出，故栈底即 LRU 端。
/// 取 256 MiB：常见 10 MB 量级 PDF 仍能吃满 20 步，大文件则改由内存封顶。
const UNDO_BUDGET_BYTES: usize = 256 * 1024 * 1024;

/// 压入快照，并按「条数上限 + 内存预算」从栈底淘汰最旧的快照。
///
/// 至少保留 1 条：单条快照自身就超预算时，也不该把撤销能力整个丢掉。
fn push_bounded(stack: &mut Vec<Vec<u8>>, bytes: Vec<u8>, limit: usize, budget: usize) {
    stack.push(bytes);
    let mut total: usize = stack.iter().map(|s| s.len()).sum();
    while stack.len() > 1 && (stack.len() > limit || total > budget) {
        total -= stack.remove(0).len();
    }
}

/// 全局 PDFium 实例。
///
/// 注意：pdfium-render 的 `thread_safe` feature 只在 `FPDF_InitLibrary` /
/// `FPDF_DestroyLibrary` 期间持有全局锁（用于保证同一时刻仅存在一个 `Pdfium` 实例），
/// 其余 `FPDF_*` 调用一律直接转发、不加任何锁。因此跨线程共享 `&Pdfium` 本身并不安全，
/// 进程内并发调用 pdfium 会触达其全局缓存/错误状态，导致偶发
/// `PdfiumLibraryInternalError(Unknown)`、堆损坏甚至访问违例。
///
/// 单次调用层面的串行化由 `pdfium_gate()` 命令层闸门保证；trait object 不声明
/// Send/Sync，故此处手动包装。
struct PdfiumHolder(Pdfium);
// SAFETY: 所有可达的 `#[tauri::command]` 都在入口获取 `pdfium_gate()`，
// 保证同一时刻只有一个线程在调用 pdfium。详见 `pdfium_gate` 的文档。
unsafe impl Sync for PdfiumHolder {}
unsafe impl Send for PdfiumHolder {}

/// pdfium 命令层闸门：串行化所有会调用 pdfium 的命令。
///
/// pdfium 是带大量进程级全局状态的 C 库，且有多个 `FPDF_*` 入口并不线程安全；
/// `pdfium-render` 的 `thread_safe` 只覆盖库的 init/destroy，不覆盖单次调用。
/// tauri 默认使用多线程 tokio 运行时，两个命令完全可能落在不同 worker 上并发执行，
/// 故必须在命令边界串行化。
///
/// 用法（**仅限 `#[tauri::command]` 函数体内**）：
/// ```ignore
/// let _gate = crate::document::pdfium_gate();
/// ```
/// 必须绑定到具名变量（写成 `let _ = ...` 会立即析构，闸门失效）。
///
/// **严禁在任何 helper（`*_logic`、`inspect`、`commit_and_return`、`load_doc` 等）
/// 内获取本闸门**：这些 helper 会被已持闸门的命令再次调用（如 `commit_and_return`
/// 需要 `get_pdfium()`），而非可重入的 `Mutex` 在同一线程重入会立即死锁。
///
/// 中毒时取回内部值继续使用，避免一次 panic 永久砖化全部命令。
pub fn pdfium_gate() -> std::sync::MutexGuard<'static, ()> {
    static GATE: Mutex<()> = Mutex::new(());
    GATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn pdfium() -> &'static Pdfium {
    static PDFIUM: OnceLock<PdfiumHolder> = OnceLock::new();
    &PDFIUM
        .get_or_init(|| {
            let dll = pdfium_library_path().expect("找不到 pdfium.dll，请检查安装");
            let bindings = Pdfium::bind_to_library(&dll)
                .or_else(|_| Pdfium::bind_to_system_library())
                .expect("加载 pdfium.dll 失败");
            PdfiumHolder(Pdfium::new(bindings))
        })
        .0
}

/// 依次尝试：资源目录、可执行文件同级、开发期 src-tauri/pdfium/。
fn pdfium_library_path() -> Option<std::path::PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let mut candidates = vec![
        exe_dir.join("pdfium.dll"),
        exe_dir.join("resources").join("pdfium.dll"),
    ];
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("pdfium").join("pdfium.dll"));
        candidates.push(cwd.join("src-tauri").join("pdfium").join("pdfium.dll"));
    }
    // 项目根（从 src-tauri 目录向上）
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("pdfium")
            .join("pdfium.dll"),
    );
    candidates.into_iter().find(|p| p.exists())
}

pub(crate) struct DocEntry {
    pub(crate) bytes: Vec<u8>,
    pub(crate) path: Option<std::path::PathBuf>,
}

/// Tauri 托管状态：文档字节表 + 每文档撤销/重做快照栈。
#[derive(Default)]
pub struct AppState {
    pub(crate) docs: Mutex<HashMap<u64, DocEntry>>,
    pub(crate) undo: Mutex<HashMap<u64, Vec<Vec<u8>>>>,
    pub(crate) redo: Mutex<HashMap<u64, Vec<Vec<u8>>>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl AppState {
    pub(crate) fn next_doc_id(&self) -> u64 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub index: u32,
    pub width: f64,
    pub height: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfo {
    pub doc_id: u64,
    pub file_name: String,
    /// 磁盘文件字节数（bytes.len() 来自打开时的读入；明文副本导出后保持原大小）。
    pub file_size_bytes: u64,
    pub page_count: u32,
    pub pages: Vec<PageInfo>,
}

/// 校验字节流可被 PDFium 解析，并返回页数与页面尺寸。
fn inspect(bytes: &[u8], password: Option<&str>) -> AppResult<(u32, Vec<PageInfo>)> {
    let pdfium = pdfium();
    let doc = pdfium.load_pdf_from_byte_slice(bytes, password)?;
    let pages = doc.pages();
    let count = pages.len() as u32;
    let mut infos = Vec::with_capacity(count as usize);
    for i in 0..count {
        let idx = u16::try_from(i).map_err(|_| AppError::PageOutOfRange)?;
        let page = pages.get(idx).map_err(|_| AppError::PageOutOfRange)?;
        infos.push(PageInfo {
            index: i,
            width: page.width().value as f64,
            height: page.height().value as f64,
        });
    }
    Ok((count, infos))
}

fn build_info(doc_id: u64, entry: &DocEntry, count: u32, pages: Vec<PageInfo>) -> DocumentInfo {
    let file_name = entry
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "未命名文档".into());
    DocumentInfo {
        doc_id,
        file_name,
        file_size_bytes: entry.bytes.len() as u64,
        page_count: count,
        pages,
    }
}

/// 打开 PDF 文档。password 用于加密文档。
#[tauri::command]
pub async fn open_document(
    state: State<'_, AppState>,
    path: String,
    password: Option<String>,
) -> AppResult<DocumentInfo> {
    let _gate = pdfium_gate();
    let bytes = fs::read(&path)?;
    let (count, pages) = inspect(&bytes, password.as_deref())?;
    let doc_id = state.next_doc_id();
    let entry = DocEntry {
        bytes,
        path: Some(std::path::PathBuf::from(&path)),
    };
    let info = build_info(doc_id, &entry, count, pages);
    state.docs.lock().unwrap().insert(doc_id, entry);
    Ok(info)
}

/// 关闭文档，释放内存。
#[tauri::command]
pub async fn close_document(state: State<'_, AppState>, doc_id: u64) -> AppResult<()> {
    state.docs.lock().unwrap().remove(&doc_id);
    state.undo.lock().unwrap().remove(&doc_id);
    state.redo.lock().unwrap().remove(&doc_id);
    Ok(())
}

/// 获取文档元数据（页数、页面尺寸）。
#[tauri::command]
pub async fn get_metadata(state: State<'_, AppState>, doc_id: u64) -> AppResult<DocumentInfo> {
    let _gate = pdfium_gate();
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    let (count, pages) = inspect(&entry.bytes, None)?;
    Ok(build_info(doc_id, entry, count, pages))
}

/// 保存文档（原子写入：先写临时文件再改名）。
/// path 为空时保存到原文件。
#[tauri::command]
pub async fn save_document(
    state: State<'_, AppState>,
    doc_id: u64,
    path: Option<String>,
) -> AppResult<DocumentInfo> {
    let _gate = pdfium_gate();
    let mut docs = state.docs.lock().unwrap();
    let entry = docs.get_mut(&doc_id).ok_or(AppError::NotFound)?;

    let target = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => entry.path.clone().ok_or(AppError::NoSavePath)?,
    };

    // 先在锁内生成新字节，再写盘
    let new_bytes = {
        let pdfium = pdfium();
        let doc = pdfium.load_pdf_from_byte_slice(&entry.bytes, None)?;
        doc.save_to_bytes()?
    };

    atomic_write(&target, &new_bytes)?;

    entry.bytes = new_bytes;
    entry.path = Some(target);
    let (count, pages) = inspect(&entry.bytes, None)?;
    Ok(build_info(doc_id, entry, count, pages))
}

/// 原子写入：写到同目录 .tmp 后 rename。
pub fn atomic_write(target: &Path, bytes: &[u8]) -> AppResult<()> {
    let tmp = {
        let mut name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "out.pdf".into());
        name.push_str(".tmp");
        target.with_file_name(name)
    };
    fs::write(&tmp, bytes)?;
    if target.exists() {
        fs::remove_file(target)?;
    }
    fs::rename(&tmp, target)?;
    Ok(())
}

/// 修改文档前调用：压入当前快照，并清空 redo 栈（标准撤销/重做语义）。
pub fn push_snapshot(state: &AppState, doc_id: u64) {
    let docs = state.docs.lock().unwrap();
    let Some(entry) = docs.get(&doc_id) else {
        return;
    };
    push_snapshot_inner(state, doc_id, entry.bytes.clone());
    drop(docs);
    // 一旦有新的修改，redo 栈失效
    state.redo.lock().unwrap().remove(&doc_id);
}

pub(crate) fn push_snapshot_inner(state: &AppState, doc_id: u64, bytes: Vec<u8>) {
    let mut undo = state.undo.lock().unwrap();
    push_bounded(
        undo.entry(doc_id).or_default(),
        bytes,
        UNDO_LIMIT,
        UNDO_BUDGET_BYTES,
    );
}

pub(crate) fn push_redo_snapshot(state: &AppState, doc_id: u64, bytes: Vec<u8>) {
    let mut redo = state.redo.lock().unwrap();
    push_bounded(
        redo.entry(doc_id).or_default(),
        bytes,
        UNDO_LIMIT,
        UNDO_BUDGET_BYTES,
    );
}

/// 撤销一步修改，返回新元数据。
#[tauri::command]
pub async fn undo_document(state: State<'_, AppState>, doc_id: u64) -> AppResult<DocumentInfo> {
    let _gate = pdfium_gate();
    // 在独立作用域内取快照，确保离开时已释放 undo 锁（锁序恒为 docs → {undo, redo}）
    let popped = {
        let mut undo = state.undo.lock().unwrap();
        undo.get_mut(&doc_id).and_then(|stack| stack.pop())
    };
    let snapshot = match popped {
        Some(snap) => snap,
        None => {
            // 文档已关闭 → NotFound；文档在但还没压过快照 → 等同于栈空
            return if state.docs.lock().unwrap().contains_key(&doc_id) {
                Err(AppError::NothingToUndo)
            } else {
                Err(AppError::NotFound)
            };
        }
    };
    let mut docs = state.docs.lock().unwrap();
    let entry = docs.get_mut(&doc_id).ok_or(AppError::NotFound)?;
    // 把撤销前的当前 bytes 推入 redo 栈，重做即回到这一步
    push_redo_snapshot(&state, doc_id, entry.bytes.clone());
    entry.bytes = snapshot;
    let (count, pages) = inspect(&entry.bytes, None)?;
    Ok(build_info(doc_id, entry, count, pages))
}

/// 重做一步（撤销的反向操作）
#[tauri::command]
pub async fn redo_document(state: State<'_, AppState>, doc_id: u64) -> AppResult<DocumentInfo> {
    let _gate = pdfium_gate();
    let popped = {
        let mut redo = state.redo.lock().unwrap();
        redo.get_mut(&doc_id).and_then(|stack| stack.pop())
    };
    let snapshot = match popped {
        Some(snap) => snap,
        None => {
            // 文档已关闭 → NotFound；文档在但还没压过快照 → 等同于栈空
            return if state.docs.lock().unwrap().contains_key(&doc_id) {
                Err(AppError::NothingToRedo)
            } else {
                Err(AppError::NotFound)
            };
        }
    };
    let mut docs = state.docs.lock().unwrap();
    let entry = docs.get_mut(&doc_id).ok_or(AppError::NotFound)?;
    // 把重做前的当前 bytes 推回 undo 栈（保证再撤销仍可用）
    push_snapshot_inner(&state, doc_id, entry.bytes.clone());
    entry.bytes = snapshot;
    let (count, pages) = inspect(&entry.bytes, None)?;
    Ok(build_info(doc_id, entry, count, pages))
}

/// 是否可撤销。
#[tauri::command]
pub async fn can_undo(state: State<'_, AppState>, doc_id: u64) -> AppResult<bool> {
    let undo = state.undo.lock().unwrap();
    Ok(undo.get(&doc_id).map(|s| !s.is_empty()).unwrap_or(false))
}

/// 是否可重做。
#[tauri::command]
pub async fn can_redo(state: State<'_, AppState>, doc_id: u64) -> AppResult<bool> {
    let redo = state.redo.lock().unwrap();
    Ok(redo.get(&doc_id).map(|s| !s.is_empty()).unwrap_or(false))
}

/// 撤销栈深度（可撤销的步数）。
#[tauri::command]
pub async fn undo_depth(state: State<'_, AppState>, doc_id: u64) -> AppResult<u32> {
    let undo = state.undo.lock().unwrap();
    Ok(undo.get(&doc_id).map(|s| s.len() as u32).unwrap_or(0))
}

/// 重做栈深度（可重做的步数）。
#[tauri::command]
pub async fn redo_depth(state: State<'_, AppState>, doc_id: u64) -> AppResult<u32> {
    let redo = state.redo.lock().unwrap();
    Ok(redo.get(&doc_id).map(|s| s.len() as u32).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    //! Unit tests for atomic file writes.
    use super::*;
    use std::env;

    /// atomic_write: writes the bytes to the target path.
    #[test]
    fn atomic_write_creates_file() {
        let dir = env::temp_dir().join("pdfe_test_atomic");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("a.pdf");
        let _ = std::fs::remove_file(&target);

        atomic_write(&target, b"hello").unwrap();
        assert!(target.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"hello".to_vec());
    }

    /// atomic_write: overwrites an existing file atomically.
    #[test]
    fn atomic_write_overwrites_existing() {
        let dir = env::temp_dir().join("pdfe_test_atomic2");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("b.pdf");
        std::fs::write(&target, b"original").unwrap();

        atomic_write(&target, b"updated").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"updated".to_vec());
    }

    /// atomic_write: cleans up the temp file on success.
    #[test]
    fn atomic_write_no_temp_left() {
        let dir = env::temp_dir().join("pdfe_test_atomic3");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("c.pdf");
        std::fs::remove_file(&target).ok();

        atomic_write(&target, b"data").unwrap();

        // The .tmp sibling should not exist
        let tmp = target.with_file_name("c.pdf.tmp");
        assert!(!tmp.exists(), "tmp file should have been renamed");
        assert!(target.exists());
    }

    /// atomic_write: empty bytes is valid.
    #[test]
    fn atomic_write_empty_bytes() {
        let dir = env::temp_dir().join("pdfe_test_atomic4");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("d.pdf");
        std::fs::remove_file(&target).ok();

        atomic_write(&target, b"").unwrap();
        assert!(target.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"".to_vec());
    }

    /// push_bounded: 条数超上限时从栈底淘汰最旧的快照。
    #[test]
    fn push_bounded_evicts_oldest_by_count() {
        let mut stack: Vec<Vec<u8>> = Vec::new();
        for i in 0..5u8 {
            push_bounded(&mut stack, vec![i], 3, usize::MAX);
        }
        assert_eq!(stack.len(), 3);
        assert_eq!(stack[0], vec![2u8], "最旧的 0 / 1 已被淘汰");
        assert_eq!(stack[2], vec![4u8]);
    }

    /// push_bounded: 条数未超但总字节超预算时，同样从栈底淘汰。
    #[test]
    fn push_bounded_evicts_oldest_by_budget() {
        let mut stack: Vec<Vec<u8>> = Vec::new();
        push_bounded(&mut stack, vec![0u8; 100], 20, 250);
        push_bounded(&mut stack, vec![1u8; 100], 20, 250);
        assert_eq!(stack.len(), 2, "200 <= 250，无需淘汰");
        push_bounded(&mut stack, vec![2u8; 100], 20, 250);
        assert_eq!(stack.len(), 2, "300 > 250，淘汰最旧一条");
        assert_eq!(stack[0][0], 1);
        assert_eq!(stack[1][0], 2);
    }

    /// push_bounded: 单条快照自身即超预算时仍保留最后一条，撤销能力不被清空。
    #[test]
    fn push_bounded_keeps_at_least_one() {
        let mut stack: Vec<Vec<u8>> = Vec::new();
        push_bounded(&mut stack, vec![0u8; 10], 20, 1);
        push_bounded(&mut stack, vec![1u8; 10], 20, 1);
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0][0], 1, "保留下来的必须是最新那条");
    }
}
