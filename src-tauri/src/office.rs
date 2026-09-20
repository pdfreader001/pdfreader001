//! M8：Office 文档 ↔ PDF 互转
//!
//! 通过探测本地 LibreOffice（soffice.exe）调用 --headless --convert-to pdf，
//! 支持 Word / Excel / PPT / RTF / ODF → PDF。
//!
//! 不支持 PDF → Office（PDF→Word 等需要专业排版恢复工具，超出 LibreOffice 能力）。

use serde::Serialize;
use std::path::Path;
use std::process::Command;
use tauri::State;

use crate::document::AppState;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeProbe {
    pub installed: bool,
    /// 探测到的 soffice 路径（若有）
    pub path: Option<String>,
    /// LibreOffice 版本字符串
    pub version: Option<String>,
}

/// 探测本机 LibreOffice 安装位置
/// 1. 标准路径：C:\Program Files\LibreOffice\program\soffice.exe 等
/// 2. 注册表探测（HKLM\SOFTWARE\LibreOffice）
/// 3. PATH 中的 soffice
#[tauri::command]
pub async fn detect_office(_state: State<'_, AppState>) -> AppResult<OfficeProbe> {
    let candidates = [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        r"C:\Program Files\LibreOffice 7\program\soffice.exe",
        // Microsoft Store 版
        r"C:\Program Files\WindowsApps\LibreOffice.Program.1\program\soffice.exe",
    ];

    // 1) 直接探测常见路径
    for path in candidates {
        if Path::new(path).is_file() {
            if let Some(v) = run_version(path) {
                return Ok(OfficeProbe {
                    installed: true,
                    path: Some(path.to_string()),
                    version: Some(v),
                });
            }
        }
    }

    // 2) 用 where/which 查 PATH
    if let Ok(out) = Command::new("where").arg("soffice").output() {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !p.is_empty() && Path::new(&p).is_file() {
                if let Some(v) = run_version(&p) {
                    return Ok(OfficeProbe {
                        installed: true,
                        path: Some(p),
                        version: Some(v),
                    });
                }
            }
        }
    }

    Ok(OfficeProbe {
        installed: false,
        path: None,
        version: None,
    })
}

fn run_version(soffice: &str) -> Option<String> {
    let out = Command::new(soffice).arg("--version").output().ok()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

/// 调用 soffice 把 Office 文档转 PDF。
/// 输出文件保存在 `output_dir` 中（用户选定），文件名与原文件同名仅后缀改 .pdf。
/// 返回实际输出文件路径。
#[tauri::command]
pub async fn convert_office_to_pdf(
    soffice_path: String,
    source: String,
    output_dir: String,
) -> AppResult<String> {
    if !Path::new(&soffice_path).is_file() {
        return Err(AppError::Internal(format!(
            "LibreOffice 可执行文件不存在：{}",
            soffice_path
        )));
    }
    if !Path::new(&source).is_file() {
        return Err(AppError::Internal(format!("源文件不存在：{}", source)));
    }
    let out = Path::new(&output_dir);
    if !out.is_dir() {
        return Err(AppError::Internal(format!(
            "输出目录不存在：{}",
            output_dir
        )));
    }

    let out_str = out.to_string_lossy().to_string();
    let status = Command::new(&soffice_path)
        .arg("--headless")
        .arg("--norestore")
        .arg("--nolockcheck")
        .arg("--nodefault")
        .arg("--convert-to")
        .arg("pdf")
        .arg("--outdir")
        .arg(&out_str)
        .arg(&source)
        .status()
        .map_err(|e| AppError::Internal(format!("启动 LibreOffice 失败：{e}")))?;

    if !status.success() {
        return Err(AppError::Internal(format!(
            "LibreOffice 退出码 {}",
            status.code().unwrap_or(-1)
        )));
    }

    // 推断输出 PDF 路径
    let src_stem = Path::new(&source)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::Internal("无法获取源文件名".into()))?;
    let pdf_path = out.join(format!("{src_stem}.pdf"));
    if !pdf_path.is_file() {
        return Err(AppError::Internal(
            "LibreOffice 未生成 PDF，请检查源文件格式是否受支持".into(),
        ));
    }
    Ok(pdf_path.to_string_lossy().into_owned())
}