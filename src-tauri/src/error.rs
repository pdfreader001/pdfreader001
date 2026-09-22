use serde::Serialize;
use std::collections::HashMap;

/// 应用统一错误类型。
/// - message 为英文（日志/调试用），用户可见文案由前端 i18n 根据 code 翻译。
/// - args 为可选变量，前端用 {name} 占位符替换。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    // ------ 基础错误 ------
    #[error("Password required to open this document")]
    Password,
    #[error("Document is damaged and cannot be parsed")]
    Damaged,
    #[error("Document not found or already closed")]
    NotFound,
    #[error("Page index out of range")]
    PageOutOfRange,
    #[error("Operation denied by document security settings")]
    Security,
    #[error("File I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Internal error: {0}")]
    Internal(String),

    // ------ 撤销/重做 ------
    #[error("No operation to undo")]
    NothingToUndo,
    #[error("No operation to redo")]
    NothingToRedo,

    // ------ 保存 ------
    #[error("Document has no original path; please specify a save location")]
    NoSavePath,

    // ------ 页面操作 ------
    #[error("No pages selected for deletion")]
    NoPagesToDelete,
    #[error("Cannot delete all pages")]
    CannotDeleteAllPages,
    #[error("No pages selected for duplication")]
    NoPagesToDuplicate,
    #[error("No pages selected to move")]
    NoPagesToMove,
    #[error("No pages selected for extraction")]
    NoPagesToExtract,
    #[error("Invalid page range: {range}")]
    InvalidPageRange { range: String },

    // ------ 合并/拆分 ------
    #[error("At least one source file is required")]
    NeedAtLeastOneFile,
    #[error("Merge result is an empty document")]
    MergeResultEmpty,
    #[error("Pages per file cannot be zero")]
    PagesPerFileZero,
    #[error("Nothing to split")]
    NothingToSplit,

    // ------ 水印 ------
    #[error("Watermark text cannot be empty")]
    WatermarkTextEmpty,
    #[error("Invalid image dimensions")]
    InvalidImageSize,
    #[error("No pages to apply watermark to")]
    NoPagesForWatermark,
    #[error("No suitable Chinese font found in system (needs msyh/simhei/simsun)")]
    NoChineseFont,

    // ------ 注释 ------
    #[error("Annotation index out of range")]
    AnnotationOutOfRange,
    #[error("Text cannot be empty")]
    TextEmpty,

    // ------ 转换 ------
    #[error("No pages selected for export")]
    NoPagesToExport,
    #[error("DPI out of range ({min}–{max}): {dpi}")]
    DpiOutOfRange { dpi: u32, min: u32, max: u32 },
    #[error("Failed to construct image")]
    ImageConstructFailed,
    #[error("No images provided")]
    NoImagesProvided,
    #[error("Failed to read image: {path}")]
    ImageReadFailed { path: String },
    #[error("Failed to write PDF")]
    PdfWriteFailed,

    // ------ 水印去除 ------
    #[error("Invalid rectangle: zero or negative size")]
    InvalidRect,
    #[error("No candidate watermarks detected")]
    NoCandidates,

    // ------ Office / 电子书（外部工具） ------
    #[error("{tool} not found. Please install it first.")]
    ToolNotFound { tool: String },
    #[error("Unsupported file format: {format}")]
    UnsupportedFormat { format: String },
    #[error("Source file not found: {path}")]
    SourceNotFound { path: String },
    #[error("Failed to start {tool}: {detail}")]
    ToolStartFailed { tool: String, detail: String },
    #[error("{tool} exited with code {code}")]
    ToolFailed { tool: String, code: i32 },
    #[error("Unable to determine source file name")]
    CannotDetermineSourceName,
    #[error("No PDF was generated; please check the source file format")]
    NoPdfGenerated,
}

impl From<pdfium_render::prelude::PdfiumError> for AppError {
    fn from(e: pdfium_render::prelude::PdfiumError) -> Self {
        use pdfium_render::prelude::{PdfiumError, PdfiumInternalError};
        match e {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
                AppError::Password
            }
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
                AppError::Damaged
            }
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FileError) => {
                AppError::NotFound
            }
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
                AppError::Security
            }
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PageError) => {
                AppError::PageOutOfRange
            }
            PdfiumError::IoError(io) => AppError::Io(io),
            other => AppError::Internal(other.to_string()),
        }
    }
}

impl AppError {
    /// 错误码，前端可据此做分支处理（如弹出密码框）和 i18n 翻译。
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Password => "password",
            AppError::Damaged => "damaged",
            AppError::NotFound => "not_found",
            AppError::PageOutOfRange => "page_out_of_range",
            AppError::Security => "security",
            AppError::Io(_) => "io",
            AppError::Internal(_) => "internal",
            AppError::NothingToUndo => "nothing_to_undo",
            AppError::NothingToRedo => "nothing_to_redo",
            AppError::NoSavePath => "no_save_path",
            AppError::NoPagesToDelete => "no_pages_to_delete",
            AppError::CannotDeleteAllPages => "cannot_delete_all_pages",
            AppError::NoPagesToDuplicate => "no_pages_to_duplicate",
            AppError::NoPagesToMove => "no_pages_to_move",
            AppError::NoPagesToExtract => "no_pages_to_extract",
            AppError::InvalidPageRange { .. } => "invalid_page_range",
            AppError::NeedAtLeastOneFile => "need_at_least_one_file",
            AppError::MergeResultEmpty => "merge_result_empty",
            AppError::PagesPerFileZero => "pages_per_file_zero",
            AppError::NothingToSplit => "nothing_to_split",
            AppError::WatermarkTextEmpty => "watermark_text_empty",
            AppError::InvalidImageSize => "invalid_image_size",
            AppError::NoPagesForWatermark => "no_pages_for_watermark",
            AppError::NoChineseFont => "no_chinese_font",
            AppError::AnnotationOutOfRange => "annotation_out_of_range",
            AppError::TextEmpty => "text_empty",
            AppError::NoPagesToExport => "no_pages_to_export",
            AppError::DpiOutOfRange { .. } => "dpi_out_of_range",
            AppError::ImageConstructFailed => "image_construct_failed",
            AppError::NoImagesProvided => "no_images_provided",
            AppError::ImageReadFailed { .. } => "image_read_failed",
            AppError::PdfWriteFailed => "pdf_write_failed",
            AppError::InvalidRect => "invalid_rect",
            AppError::NoCandidates => "no_candidates",
            AppError::ToolNotFound { .. } => "tool_not_found",
            AppError::UnsupportedFormat { .. } => "unsupported_format",
            AppError::SourceNotFound { .. } => "source_not_found",
            AppError::ToolStartFailed { .. } => "tool_start_failed",
            AppError::ToolFailed { .. } => "tool_failed",
            AppError::CannotDetermineSourceName => "cannot_determine_source_name",
            AppError::NoPdfGenerated => "no_pdf_generated",
        }
    }

    /// 错误变量，前端用于 {name} 占位符替换。
    pub fn args(&self) -> HashMap<String, String> {
        let mut m = HashMap::new();
        match self {
            AppError::Io(e) => {
                m.insert("detail".into(), e.to_string());
            }
            AppError::Internal(msg) => {
                m.insert("detail".into(), msg.clone());
            }
            AppError::InvalidPageRange { range } => {
                m.insert("range".into(), range.clone());
            }
            AppError::DpiOutOfRange { dpi, min, max } => {
                m.insert("dpi".into(), dpi.to_string());
                m.insert("min".into(), min.to_string());
                m.insert("max".into(), max.to_string());
            }
            AppError::ToolNotFound { tool } => {
                m.insert("tool".into(), tool.clone());
            }
            AppError::UnsupportedFormat { format } => {
                m.insert("format".into(), format.clone());
            }
            AppError::SourceNotFound { path } => {
                m.insert("path".into(), path.clone());
            }
            AppError::ToolStartFailed { tool, detail } => {
                m.insert("tool".into(), tool.clone());
                m.insert("detail".into(), detail.clone());
            }
            AppError::ToolFailed { tool, code } => {
                m.insert("tool".into(), tool.clone());
                m.insert("code".into(), code.to_string());
            }
            AppError::ImageReadFailed { path } => {
                m.insert("path".into(), path.clone());
            }
            _ => {}
        }
        m
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let has_args = !matches!(
            self,
            AppError::Password
                | AppError::Damaged
                | AppError::NotFound
                | AppError::PageOutOfRange
                | AppError::Security
                | AppError::NothingToUndo
                | AppError::NothingToRedo
                | AppError::NoSavePath
                | AppError::NoPagesToDelete
                | AppError::CannotDeleteAllPages
                | AppError::NoPagesToDuplicate
                | AppError::NoPagesToMove
                | AppError::NoPagesToExtract
                | AppError::NeedAtLeastOneFile
                | AppError::MergeResultEmpty
                | AppError::PagesPerFileZero
                | AppError::NothingToSplit
                | AppError::WatermarkTextEmpty
                | AppError::InvalidImageSize
                | AppError::NoPagesForWatermark
                | AppError::NoChineseFont
                | AppError::AnnotationOutOfRange
                | AppError::TextEmpty
                | AppError::NoPagesToExport
                | AppError::ImageConstructFailed
                | AppError::NoImagesProvided
                | AppError::PdfWriteFailed
                | AppError::InvalidRect
                | AppError::NoCandidates
                | AppError::CannotDetermineSourceName
                | AppError::NoPdfGenerated
        );
        let fields = if has_args { 3 } else { 2 };
        let mut s = serializer.serialize_struct("AppError", fields)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        if has_args {
            s.serialize_field("args", &self.args())?;
        }
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
