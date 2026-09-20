use serde::Serialize;

/// 应用统一错误类型：可序列化为前端可读的中文消息。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("文档需要密码：请输入正确的打开密码")]
    Password,
    #[error("文档已损坏：PDF 格式无法解析")]
    Damaged,
    #[error("文档不存在或已被关闭")]
    NotFound,
    #[error("页码超出范围")]
    PageOutOfRange,
    #[error("安全限制：文档权限设置禁止此操作")]
    Security,
    #[error("文件读写失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("内部错误：{0}")]
    Internal(String),
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
    /// 错误码，前端可据此做分支处理（如弹出密码框）。
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Password => "password",
            AppError::Damaged => "damaged",
            AppError::NotFound => "not_found",
            AppError::PageOutOfRange => "page_out_of_range",
            AppError::Security => "security",
            AppError::Io(_) => "io",
            AppError::Internal(_) => "internal",
        }
    }

    fn message(&self) -> String {
        match self {
            AppError::Io(e) => format!("文件读写失败：{e}"),
            AppError::Internal(m) => format!("内部错误：{m}"),
            other => other.to_string(),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.message())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
