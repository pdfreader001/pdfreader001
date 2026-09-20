//! M10：电子书格式互转（EPUB / MOBI / AZW3 / FB2 / HTML / RTF 等 ↔ PDF）
//!
//! 通过探测本机 Calibre（ebook-convert）实现。
//! Calibre 是事实标准的电子书转换工具，支持数十种格式互转。
//! 若未安装则提示用户下载：https://calibre-ebook.com/download
//!
//! 注：Mobi/AZW3 等亚马逊格式本身是出版业的事实标准，
//! Amazon 官方从未公开过它们的解码规范，Calibre 是通过逆向工程实现的。

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use tauri::State;

use crate::document::AppState;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EbookToolProbe {
    pub installed: bool,
    /// Calibre 的 ebook-convert 可执行文件路径
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertEbookOpts {
    /// 源文件路径（epub/mobi/azw3/azw/fb2/lit/html/rtf/docx 等）
    pub source: String,
    /// 输出 PDF 路径（用户选定）
    pub output: String,
    /// 可选：作者（写入 PDF metadata）
    pub author: Option<String>,
    /// 可选：标题（写入 PDF metadata）
    pub title: Option<String>,
}

fn probe_calibre() -> Option<(String, Option<String>)> {
    // 1) 常见安装路径
    let candidates = [
        r"C:\Program Files\Calibre2\ebook-convert.exe",
        r"C:\Program Files (x86)\Calibre2\ebook-convert.exe",
        r"C:\Program Files\Calibre\ebook-convert.exe",
        r"C:\Program Files (x86)\Calibre\ebook-convert.exe",
    ];
    for p in candidates {
        if Path::new(p).is_file() {
            if let Ok(out) = Command::new(p).arg("--version").output() {
                if out.status.success() {
                    let v = String::from_utf8_lossy(&out.stdout)
                        .trim()
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_string();
                    return Some((p.to_string(), if v.is_empty() { None } else { Some(v) }));
                }
            }
            return Some((p.to_string(), None));
        }
    }
    // 2) PATH 中 ebook-convert
    if let Ok(out) = Command::new("where").arg("ebook-convert").output() {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !p.is_empty() && Path::new(&p).is_file() {
                if let Ok(out2) = Command::new(&p).arg("--version").output() {
                    if out2.status.success() {
                        let v = String::from_utf8_lossy(&out2.stdout)
                            .trim()
                            .lines()
                            .next()
                            .unwrap_or("")
                            .to_string();
                        return Some((p, if v.is_empty() { None } else { Some(v) }));
                    }
                }
                return Some((p, None));
            }
        }
    }
    None
}

#[tauri::command]
pub async fn detect_ebook_tools(_state: State<'_, AppState>) -> AppResult<EbookToolProbe> {
    if let Some((path, version)) = probe_calibre() {
        Ok(EbookToolProbe {
            installed: true,
            path: Some(path),
            version,
        })
    } else {
        Ok(EbookToolProbe {
            installed: false,
            path: None,
            version: None,
        })
    }
}

/// 调用 Calibre ebook-convert 把任意电子书格式转 PDF。
/// 返回写入的 PDF 路径。
#[tauri::command]
pub async fn convert_ebook_to_pdf(
    tool_path: String,
    opts: ConvertEbookOpts,
) -> AppResult<String> {
    if !Path::new(&tool_path).is_file() {
        return Err(AppError::Internal(format!(
            "Calibre ebook-convert 不存在：{}",
            tool_path
        )));
    }
    if !Path::new(&opts.source).is_file() {
        return Err(AppError::Internal(format!("源文件不存在：{}", opts.source)));
    }
    let out_path = Path::new(&opts.output);
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            return Err(AppError::Internal(format!(
                "输出目录不存在：{}",
                parent.display()
            )));
        }
    }

    let mut cmd = Command::new(&tool_path);
    cmd.arg(&opts.source).arg(&opts.output);
    if let Some(title) = opts.title.as_ref().filter(|s| !s.is_empty()) {
        cmd.arg("--title").arg(title);
    }
    if let Some(author) = opts.author.as_ref().filter(|s| !s.is_empty()) {
        cmd.arg("--authors").arg(author);
    }
    // 让 Calibre 不要弹 GUI
    cmd.env("CALIBRE_USE_SYSTEM_THUMBNAILERS", "1");
    let status = cmd
        .status()
        .map_err(|e| AppError::Internal(format!("启动 ebook-convert 失败：{e}")))?;
    if !status.success() {
        return Err(AppError::Internal(format!(
            "ebook-convert 退出码 {}",
            status.code().unwrap_or(-1)
        )));
    }
    if !out_path.is_file() {
        return Err(AppError::Internal("未生成 PDF，请检查源文件格式".into()));
    }
    Ok(out_path.to_string_lossy().into_owned())
}