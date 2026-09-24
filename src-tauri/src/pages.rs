use std::fs;
use std::path::PathBuf;

use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::document::{atomic_write, pdfium as get_pdfium, push_snapshot, AppState, DocumentInfo, PageInfo};
use crate::error::{AppError, AppResult};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeSource {
    pub path: String,
    pub ranges: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkNode {
    pub title: String,
    pub page_index: u32,
    pub level: u32,
    pub children: Vec<BookmarkNode>,
}

pub(crate) fn load_doc<'a>(pdfium: &'a Pdfium, bytes: &'a [u8]) -> AppResult<PdfDocument<'a>> {
    Ok(pdfium.load_pdf_from_byte_slice(bytes, None)?)
}

fn build_info_from_doc(
    doc_id: u64,
    doc: &PdfDocument,
    bytes: &[u8],
    path: Option<PathBuf>,
) -> AppResult<DocumentInfo> {
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
    let file_name = path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "未命名文档".into());
    Ok(DocumentInfo {
        doc_id,
        file_name,
        file_size_bytes: bytes.len() as u64,
        page_count: count,
        pages: infos,
    })
}

pub(crate) fn commit_and_return(
    state: &AppState,
    doc_id: u64,
    new_bytes: Vec<u8>,
) -> AppResult<DocumentInfo> {
    let mut docs = state.docs.lock().unwrap();
    let entry = docs.get_mut(&doc_id).ok_or(AppError::NotFound)?;
    entry.bytes = new_bytes;
    let pdfium = get_pdfium();
    let doc = load_doc(pdfium, &entry.bytes)?;
    build_info_from_doc(doc_id, &doc, &entry.bytes, entry.path.clone())
}

fn to_u16(i: u32) -> AppResult<u16> {
    u16::try_from(i).map_err(|_| AppError::PageOutOfRange)
}

pub(crate) fn normalize_indices(pages: &[u32], total: u32) -> AppResult<Vec<u16>> {
    let mut out = Vec::with_capacity(pages.len());
    for &p in pages {
        if p >= total {
            return Err(AppError::PageOutOfRange);
        }
        out.push(to_u16(p)?);
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

#[tauri::command]
pub async fn rotate_pages(
    state: State<'_, AppState>,
    doc_id: u64,
    pages: Vec<u32>,
    delta_deg: i32,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        let total = doc.pages().len() as u32;
        let indices = normalize_indices(&pages, total)?;
        {
            let pages = doc.pages();
            for idx in indices {
                let mut page = pages.get(idx)?;
                let current = page.rotation()?;
                let current_deg = match current {
                    PdfPageRenderRotation::None => 0,
                    PdfPageRenderRotation::Degrees90 => 90,
                    PdfPageRenderRotation::Degrees180 => 180,
                    PdfPageRenderRotation::Degrees270 => 270,
                };
                let next = ((current_deg + delta_deg) % 360 + 360) % 360;
                let rot = match next {
                    0 => PdfPageRenderRotation::None,
                    90 => PdfPageRenderRotation::Degrees90,
                    180 => PdfPageRenderRotation::Degrees180,
                    270 => PdfPageRenderRotation::Degrees270,
                    _ => PdfPageRenderRotation::None,
                };
                page.set_rotation(rot);
            }
        }
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

#[tauri::command]
pub async fn delete_pages(
    state: State<'_, AppState>,
    doc_id: u64,
    pages: Vec<u32>,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        let total = doc.pages().len() as u32;
        let indices = normalize_indices(&pages, total)?;
        if indices.is_empty() {
            return Err(AppError::NoPagesToDelete);
        }
        if indices.len() as u32 >= total {
            return Err(AppError::CannotDeleteAllPages);
        }
        for idx in indices.iter().rev() {
            let page = doc.pages().get(*idx)?;
            page.delete()?;
        }
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

#[tauri::command]
pub async fn duplicate_pages(
    state: State<'_, AppState>,
    doc_id: u64,
    pages: Vec<u32>,
    dest_index: u32,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let src_doc = load_doc(pdfium, &entry.bytes)?;
        let total = src_doc.pages().len() as u32;
        if dest_index > total {
            return Err(AppError::PageOutOfRange);
        }
        let indices = normalize_indices(&pages, total)?;
        if indices.is_empty() {
            return Err(AppError::NoPagesToDuplicate);
        }
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        let dest = to_u16(dest_index)?;
        let mut insert_at = dest;
        for &idx in &indices {
            doc.pages_mut()
                .copy_page_from_document(&src_doc, idx, insert_at)?;
            insert_at = insert_at.saturating_add(1);
        }
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

#[tauri::command]
pub async fn insert_blank_page(
    state: State<'_, AppState>,
    doc_id: u64,
    at_index: u32,
    width: f64,
    height: f64,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        let total = doc.pages().len() as u32;
        if at_index > total {
            return Err(AppError::PageOutOfRange);
        }
        let size = PdfPagePaperSize::from_points(
            PdfPoints::new(width as f32),
            PdfPoints::new(height as f32),
        );
        doc.pages_mut()
            .create_page_at_index(size, to_u16(at_index)?)?;
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

#[tauri::command]
pub async fn reorder_pages(
    state: State<'_, AppState>,
    doc_id: u64,
    from_indices: Vec<u32>,
    to_index: u32,
) -> AppResult<DocumentInfo> {
    push_snapshot(&state, doc_id);
    let pdfium = get_pdfium();
    let new_bytes = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let mut doc = load_doc(pdfium, &entry.bytes)?;
        let total = doc.pages().len() as u32;
        if to_index > total {
            return Err(AppError::PageOutOfRange);
        }
        let indices = normalize_indices(&from_indices, total)?;
        if indices.is_empty() {
            return Err(AppError::NoPagesToMove);
        }
        let count = indices.len() as u32;
        let first_from = indices[0] as u32;
        let last_from = indices[indices.len() - 1] as u32;
        let dest = if to_index > last_from {
            to_index - count
        } else {
            to_index
        };
        if dest == first_from {
            return commit_and_return(&state, doc_id, entry.bytes.clone());
        }
        let mut tmp = pdfium.create_new_pdf()?;
        for &idx in &indices {
            let src_page = doc.pages().get(idx)?;
            let w = src_page.width();
            let h = src_page.height();
            let size = PdfPagePaperSize::from_points(w, h);
            let tgt_idx = tmp.pages().len();
            tmp.pages_mut().create_page_at_index(size, tgt_idx)?;
        }
        for (i, &idx) in indices.iter().enumerate() {
            tmp.pages_mut()
                .copy_page_from_document(&doc, idx, i as u16)?;
        }
        for idx in indices.iter().rev() {
            let page = doc.pages().get(*idx)?;
            page.delete()?;
        }
        let dest_u16 = to_u16(dest)?;
        doc.pages_mut()
            .copy_page_range_from_document(&tmp, 0..=(count as u16 - 1), dest_u16)?;
        doc.save_to_bytes()?
    };
    commit_and_return(&state, doc_id, new_bytes)
}

#[tauri::command]
pub async fn extract_pages(
    state: State<'_, AppState>,
    doc_id: u64,
    pages: Vec<u32>,
    output_path: Option<String>,
) -> AppResult<DocumentInfo> {
    let pdfium = get_pdfium();
    let (new_bytes, new_path) = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let doc = load_doc(pdfium, &entry.bytes)?;
        let total = doc.pages().len() as u32;
        let indices = normalize_indices(&pages, total)?;
        if indices.is_empty() {
            return Err(AppError::NoPagesToExtract);
        }
        let mut new_doc = pdfium.create_new_pdf()?;
        for (i, &idx) in indices.iter().enumerate() {
            let src_page = doc.pages().get(idx)?;
            let size = PdfPagePaperSize::from_points(src_page.width(), src_page.height());
            new_doc.pages_mut().create_page_at_index(size, i as u16)?;
        }
        for (i, &idx) in indices.iter().enumerate() {
            new_doc
                .pages_mut()
                .copy_page_from_document(&doc, idx, i as u16)?;
        }
        let bytes = new_doc.save_to_bytes()?;
        let path = output_path.map(PathBuf::from);
        (bytes, path)
    };
    if let Some(ref p) = new_path {
        atomic_write(p, &new_bytes)?;
    }
    let new_doc_id = state.next_doc_id();
    let entry = crate::document::DocEntry {
        bytes: new_bytes,
        path: new_path.clone(),
    };
    let info = {
        let pdfium = get_pdfium();
        let doc = load_doc(pdfium, &entry.bytes)?;
        build_info_from_doc(new_doc_id, &doc, &entry.bytes, entry.path.clone())?
    };
    state.docs.lock().unwrap().insert(new_doc_id, entry);
    Ok(info)
}

#[tauri::command]
pub async fn merge_documents(
    state: State<'_, AppState>,
    sources: Vec<MergeSource>,
    output_path: Option<String>,
) -> AppResult<DocumentInfo> {
    if sources.is_empty() {
        return Err(AppError::NeedAtLeastOneFile);
    }
    let pdfium = get_pdfium();
    let mut merged = pdfium.create_new_pdf()?;
    for src in &sources {
        let bytes = fs::read(&src.path)?;
        let doc = load_doc(pdfium, &bytes)?;
        let total = doc.pages().len();
        let dest_idx = merged.pages().len();
        if let Some(ref ranges) = src.ranges {
            if ranges.trim().is_empty() {
                merged
                    .pages_mut()
                    .copy_page_range_from_document(&doc, 0..=(total - 1), dest_idx)?;
            } else {
                merged
                    .pages_mut()
                    .copy_pages_from_document(&doc, ranges, dest_idx)?;
            }
        } else {
            merged
                .pages_mut()
                .copy_page_range_from_document(&doc, 0..=(total - 1), dest_idx)?;
        }
    }
    if merged.pages().len() == 0 {
        return Err(AppError::MergeResultEmpty);
    }
    let new_bytes = merged.save_to_bytes()?;
    let path = output_path.map(PathBuf::from);
    if let Some(ref p) = path {
        atomic_write(p, &new_bytes)?;
    }
    let new_doc_id = state.next_doc_id();
    let entry = crate::document::DocEntry {
        bytes: new_bytes,
        path: path.clone(),
    };
    let info = {
        let pdfium = get_pdfium();
        let doc = load_doc(pdfium, &entry.bytes)?;
        build_info_from_doc(new_doc_id, &doc, &entry.bytes, entry.path.clone())?
    };
    state.docs.lock().unwrap().insert(new_doc_id, entry);
    Ok(info)
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode", content = "payload")]
pub enum SplitMode {
    EveryN { n: u32 },
    Ranges { ranges: String },
    ByBookmark { level: Option<u32> },
    Selected { pages: Vec<u32> },
}

pub(crate) fn parse_ranges(spec: &str, total: u32) -> AppResult<Vec<(u32, u32)>> {
    let mut out = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            let start: u32 = a
                .trim()
                .parse()
                .map_err(|_| AppError::InvalidPageRange { range: part.to_string() })?;
            let end: u32 = b
                .trim()
                .parse()
                .map_err(|_| AppError::InvalidPageRange { range: part.to_string() })?;
            if start == 0 || end == 0 || start > end || end > total {
                return Err(AppError::InvalidPageRange { range: part.to_string() });
            }
            out.push((start - 1, end - 1));
        } else {
            let n: u32 = part
                .parse()
                .map_err(|_| AppError::InvalidPageRange { range: part.to_string() })?;
            if n == 0 || n > total {
                return Err(AppError::InvalidPageRange { range: part.to_string() });
            }
            out.push((n - 1, n - 1));
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn split_document(
    state: State<'_, AppState>,
    doc_id: u64,
    mode: SplitMode,
    output_dir: String,
) -> AppResult<Vec<String>> {
    let pdfium = get_pdfium();
    let (entry_bytes, total) = {
        let docs = state.docs.lock().unwrap();
        let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
        let doc = load_doc(pdfium, &entry.bytes)?;
        (entry.bytes.clone(), doc.pages().len() as u32)
    };
    let doc = load_doc(pdfium, &entry_bytes)?;
    let dir = PathBuf::from(&output_dir);
    fs::create_dir_all(&dir)?;

    let groups: Vec<Vec<u16>> = match mode {
        SplitMode::EveryN { n } => {
            if n == 0 {
                return Err(AppError::PagesPerFileZero);
            }
            let mut groups = Vec::new();
            let mut i = 0u32;
            while i < total {
                let end = (i + n).min(total);
                let mut g = Vec::new();
                for j in i..end {
                    g.push(to_u16(j)?);
                }
                groups.push(g);
                i = end;
            }
            groups
        }
        SplitMode::Ranges { ranges } => {
            let parsed = parse_ranges(&ranges, total)?;
            parsed
                .into_iter()
                .map(|(a, b)| {
                    let mut g = Vec::new();
                    for j in a..=b {
                        g.push(to_u16(j)?);
                    }
                    Ok(g)
                })
                .collect::<AppResult<Vec<_>>>()?
        }
        SplitMode::ByBookmark { level } => {
            let max_level = level.unwrap_or(1);
            let mut splits: Vec<u32> = Vec::new();
            if let Some(root) = doc.bookmarks().root() {
                collect_bookmark_pages(&root, 0, max_level, &mut splits);
            }
            splits.sort_unstable();
            splits.dedup();
            if splits.is_empty() || splits[0] != 0 {
                splits.insert(0, 0);
            }
            splits.push(total);
            let mut groups = Vec::new();
            for w in splits.windows(2) {
                let start = w[0];
                let end = w[1];
                if start < end {
                    let mut g = Vec::new();
                    for j in start..end {
                        g.push(to_u16(j)?);
                    }
                    groups.push(g);
                }
            }
            groups
        }
        SplitMode::Selected { pages } => {
            let indices = normalize_indices(&pages, total)?;
            indices
                .into_iter()
                .map(|i| Ok(vec![i]))
                .collect::<AppResult<Vec<_>>>()?
        }
    };

    if groups.is_empty() {
        return Err(AppError::NothingToSplit);
    }

    let mut outputs = Vec::new();
    for (idx, group) in groups.iter().enumerate() {
        let mut new_doc = pdfium.create_new_pdf()?;
        for (i, &pg) in group.iter().enumerate() {
            let src_page = doc.pages().get(pg)?;
            let size = PdfPagePaperSize::from_points(src_page.width(), src_page.height());
            new_doc.pages_mut().create_page_at_index(size, i as u16)?;
        }
        for (i, &pg) in group.iter().enumerate() {
            new_doc
                .pages_mut()
                .copy_page_from_document(&doc, pg, i as u16)?;
        }
        let bytes = new_doc.save_to_bytes()?;
        let file_name = format!("part_{:03}.pdf", idx + 1);
        let path = dir.join(file_name);
        atomic_write(&path, &bytes)?;
        outputs.push(path.to_string_lossy().into_owned());
    }
    Ok(outputs)
}

fn collect_bookmark_pages(
    bookmark: &PdfBookmark,
    current_level: u32,
    max_level: u32,
    out: &mut Vec<u32>,
) {
    if current_level < max_level {
        if let Some(dest) = bookmark.destination() {
            if let Ok(page_idx) = dest.page_index() {
                out.push(page_idx as u32);
            }
        }
    }
    if let Some(child) = bookmark.first_child() {
        collect_bookmark_pages(&child, current_level + 1, max_level, out);
    }
    if let Some(sibling) = bookmark.next_sibling() {
        collect_bookmark_pages(&sibling, current_level, max_level, out);
    }
}

#[tauri::command]
pub async fn get_bookmarks(
    state: State<'_, AppState>,
    doc_id: u64,
) -> AppResult<Vec<BookmarkNode>> {
    let docs = state.docs.lock().unwrap();
    let entry = docs.get(&doc_id).ok_or(AppError::NotFound)?;
    let pdfium = get_pdfium();
    let doc = load_doc(pdfium, &entry.bytes)?;
    let mut result = Vec::new();
    if let Some(root) = doc.bookmarks().root() {
        build_bookmark_tree(&root, 0, &mut result);
    }
    Ok(result)
}

fn build_bookmark_tree(bookmark: &PdfBookmark, level: u32, out: &mut Vec<BookmarkNode>) {
    let title = bookmark.title().unwrap_or_default();
    let page_index = bookmark
        .destination()
        .and_then(|d| d.page_index().ok())
        .map(|p| p as u32)
        .unwrap_or(0);
    let mut children = Vec::new();
    if let Some(child) = bookmark.first_child() {
        build_bookmark_children(&child, level + 1, &mut children);
    }
    out.push(BookmarkNode {
        title,
        page_index,
        level,
        children,
    });
    if let Some(sibling) = bookmark.next_sibling() {
        build_bookmark_tree(&sibling, level, out);
    }
}

fn build_bookmark_children(bookmark: &PdfBookmark, level: u32, out: &mut Vec<BookmarkNode>) {
    let title = bookmark.title().unwrap_or_default();
    let page_index = bookmark
        .destination()
        .and_then(|d| d.page_index().ok())
        .map(|p| p as u32)
        .unwrap_or(0);
    let mut children = Vec::new();
    if let Some(child) = bookmark.first_child() {
        build_bookmark_children(&child, level + 1, &mut children);
    }
    out.push(BookmarkNode {
        title,
        page_index,
        level,
        children,
    });
    if let Some(sibling) = bookmark.next_sibling() {
        build_bookmark_children(&sibling, level, out);
    }
}

#[cfg(test)]
mod tests {
    // Unit tests: page-range parsing + index normalization.
    use super::*;

    /// parse_ranges: single page
    #[test]
    fn parse_ranges_single_page() {
        let r = parse_ranges("3", 10).unwrap();
        assert_eq!(r, vec![(2, 2)]);
    }

    /// parse_ranges: contiguous range
    #[test]
    fn parse_ranges_contiguous() {
        let r = parse_ranges("1-3", 10).unwrap();
        assert_eq!(r, vec![(0, 2)]);
    }

    /// parse_ranges: multiple segments
    #[test]
    fn parse_ranges_multiple_segments() {
        let r = parse_ranges("1-2,5,7-9", 10).unwrap();
        assert_eq!(r, vec![(0, 1), (4, 4), (6, 8)]);
    }

    /// parse_ranges: tolerates surrounding whitespace
    #[test]
    fn parse_ranges_trims_whitespace() {
        let r = parse_ranges(" 1 , 3 ,  5-7 ", 10).unwrap();
        assert_eq!(r, vec![(0, 0), (2, 2), (4, 6)]);
    }

    /// parse_ranges: 0 is rejected (1-based)
    #[test]
    fn parse_ranges_zero_rejected() {
        assert!(parse_ranges("0", 10).is_err());
        assert!(parse_ranges("0-3", 10).is_err());
    }

    /// parse_ranges: out-of-range and inverted ranges
    #[test]
    fn parse_ranges_out_of_range() {
        assert!(parse_ranges("11", 10).is_err());
        assert!(parse_ranges("1-11", 10).is_err());
        assert!(parse_ranges("5-3", 10).is_err(), "start > end should fail");
    }

    /// parse_ranges: invalid numbers
    #[test]
    fn parse_ranges_invalid_number() {
        assert!(parse_ranges("abc", 10).is_err());
        assert!(parse_ranges("1-xyz", 10).is_err());
    }

    /// parse_ranges: empty string yields empty vector
    #[test]
    fn parse_ranges_empty_string() {
        let r = parse_ranges("", 10).unwrap();
        assert!(r.is_empty());
    }

    /// parse_ranges: consecutive commas are skipped
    #[test]
    fn parse_ranges_consecutive_commas() {
        let r = parse_ranges("1,,3", 10).unwrap();
        assert_eq!(r, vec![(0, 0), (2, 2)]);
    }

    /// to_u16: normal values
    #[test]
    fn to_u16_normal() {
        assert_eq!(to_u16(0).unwrap(), 0);
        assert_eq!(to_u16(123).unwrap(), 123);
        assert_eq!(to_u16(u16::MAX as u32).unwrap(), u16::MAX);
    }

    /// to_u16: overflow is rejected
    #[test]
    fn to_u16_overflow() {
        assert!(to_u16(u16::MAX as u32 + 1).is_err());
        assert!(to_u16(u32::MAX).is_err());
    }

    /// normalize_indices: sort + dedup
    #[test]
    fn normalize_indices_sort_dedup() {
        let v = normalize_indices(&[5, 2, 5, 0, 2, 8], 10).unwrap();
        assert_eq!(v, vec![0, 2, 5, 8]);
    }

    /// normalize_indices: out-of-range
    #[test]
    fn normalize_indices_out_of_range() {
        assert!(normalize_indices(&[0, 5, 10], 10).is_err());
        assert!(normalize_indices(&[0, 100], 10).is_err());
    }

    /// normalize_indices: empty
    #[test]
    fn normalize_indices_empty() {
        let v = normalize_indices(&[], 10).unwrap();
        assert!(v.is_empty());
    }
}
