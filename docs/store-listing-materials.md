# PDFe · Microsoft Store 上架素材（M7 待办项）

本文只产出**规格清单 + 截图脚本 + 商店文案草稿**，图片与最终提交都需人工完成。
实际提交动作仍卡在 Partner Center 账号（见 [实施计划](superpowers/plans/2026-09-18-implementation-plan.md) M7 未勾选项）。

## 0. 事实基线

来自 [tauri.conf.json](../src-tauri/tauri.conf.json) 与 [AppxManifest.xml](../src-tauri/msix/AppxManifest.xml)：

| 项 | 值 |
|----|----|
| 产品名 | PDFe（`productName` = `pdfe`，窗口标题 `PDFe`） |
| 版本 | `0.1.0` |
| 标识 | `com.konnyyuan.pdfe` |
| MSIX Identity | `Name="konnyyuan.pdfe"`（**上架需改为 Partner Center 分配的产品标识**） |
| 打包形态 | MSIX 包装的 full-trust Win32（`Windows.FullTrustApplication` + `rescap:runFullTrust`） |
| 最低系统 | Windows 10 1809（`10.0.17763.0`），`MaxVersionTested` = Windows 11 22H2（`10.0.22621.0`） |
| 包内语言 | `zh-CN`、`en-US` |
| 界面语言 | 中 / 英 / 日（`Ctrl+Shift+L` 循环，见 [useShortcuts.ts](../src/hooks/useShortcuts.ts#L28)） |
| 窗口 | 默认 1280×800，最小 960×640 |

两处需在提交前决策：
1. **包内语言只有 zh-CN / en-US，但界面支持日语**——若要日语一览，可在 Partner Center 作为「其他一览语言」添加（包不变），或后续把 `ja-JP` 加进清单 `Resources`。
2. **Identity 与 Publisher**：上架必须换成 Partner Center 分配值，并由微软重签；本地自签名证书只用于侧载（见 README「打包」一节）。

## 1. 图标资产

### 1.1 包内磁贴（已就绪，无需改动）

`src-tauri/icons/` 下 Windows 磁贴图标已由 Tauri 生成，实测像素如下：

| 文件 | 实测尺寸 |
|------|----------|
| `StoreLogo.png` | 50×50 |
| `Square44x44Logo.png` | 44×44 |
| `Square71x71Logo.png` | 71×71 |
| `Square89x89Logo.png` | 89×89 |
| `Square107x107Logo.png` | 107×107 |
| `Square142x142Logo.png` | 142×142 |
| `Square150x150Logo.png` | 150×150 |
| `Square284x284Logo.png` | 284×284 |
| `Square310x310Logo.png` | 310×310 |
| `Wide310x150Logo.png` | 310×150 |
| `icon.png` | 512×512（多用途源图） |

### 1.2 Store 一览所需（**待产出**）

| 资产 | 规格 | 状态 |
|------|------|------|
| 1:1 应用磁贴图标 | PNG，**300×300** | **已产出**——`docs/store-listing/StoreLogo-300x300.png`（实测 300×300，14.5 KB，由 `icon.png` 512×512 双三次缩放） |
| 16:9 超级英雄图 | PNG，1920×1080（或 3840×2160） | 可选；**不得含文字或应用 UI**，不要用图库照片 |

注意：**不要**把包内 50×50 的 `StoreLogo.png` 当一览图上传，会明显模糊。

生成 300×300 的一行命令（PowerShell，需 System.Drawing）：

```powershell
Add-Type -AssemblyName System.Drawing
$src = [System.Drawing.Image]::FromFile("src-tauri\icons\icon.png")
$dst = New-Object System.Drawing.Bitmap 300,300
$g = [System.Drawing.Graphics]::FromImage($dst)
$g.InterpolationMode = "HighQualityBicubic"
$g.DrawImage($src, 0, 0, 300, 300)
$g.Dispose(); $dst.Save("docs\store-listing\StoreLogo-300x300.png", [System.Drawing.Imaging.ImageFormat]::Png)
$dst.Dispose(); $src.Dispose()
```

## 2. 截图脚本

### 2.1 硬性规格（来自 Microsoft Learn）

- PNG（横向或纵向），单文件 ≤ 50 MB
- 桌面设备族：**1366×768 或更大**，支持 4K（3840×2160）
- 桌面截图最多 10 张，**至少 1 张必填**；官方建议 5–8 张、每个设备族至少 4 张
- 每张可配 ≤ **200 字符**说明文字，按上传顺序展示
- 构图：关键视觉与文字放在**上 2/3**（底部可能被文字覆盖层遮挡）；不要自行叠加 logo／图标／营销文案；避免极亮极暗或高对比条纹

### 2.2 拍摄前准备

1. **显示缩放设 100%**（125%/150% 下截到的物理像素数不等于逻辑分辨率，容易不达标）。
2. 分辨率设 1920×1080（或 4K），应用窗口**最大化**——默认窗口 1280×800 宽度不足 1366，直接截会不达标。
3. 准备三类真实文档（不要用 `tests/` 里合成的 `large_100.pdf` / `large_500.pdf`，画面无代表性）：
   - 多页图文 PDF，≥10 页、带目录书签（用于阅读/搜索/批注/页面管理）
   - 带重复水印的 PDF（用于水印去除）
   - 带 AcroForm 字段的 PDF（用于表单）
4. 截图方式：100% 缩放下按 `Win + PrtScn`（整屏 → `图片\屏幕截图`），或 `Win+Shift+S` 矩形截图后另存。**逐张核对像素尺寸**（右键属性 → 详细信息）。
5. 命名：`01-reader-zh.png`、`01-reader-en.png` …（中英各一套，Store 一览按语言分别上传图片）。

### 2.3 建议顺序（8 张）

界面元素取自 [Toolbar.tsx](../src/components/Toolbar.tsx)、[Rail.tsx](../src/components/Rail.tsx)、[StatusBar.tsx](../src/components/StatusBar.tsx)。

| # | 场景 | 复现步骤 | 画面应包含 | 说明（中） | 说明（EN） |
|---|------|----------|------------|-----------|-----------|
| 1 | 阅读主界面 | 打开多页图文 PDF，选中连续视图 | 顶部工具栏（打开/撤销/重做 + 9 个工具）、左栏缩略图、正文、底部状态栏（文件名 · 共 N 页、第 x / N、缩放百分比） | 打开即读：连续滚动、缩略图导航、深色/浅色主题 | Open and read: continuous scrolling, thumbnail navigation, light/dark themes |
| 2 | 全文搜索与高亮 | `Ctrl+F` 打开搜索面板，输入命中词 | 搜索面板命中列表 + 页面上高亮块 | 全文搜索，命中结果在页面直接高亮并逐条跳转 | Full-text search with on-page highlighting and jump-to-hit |
| 3 | 批注 | 打开「编辑」，在页面上落一个高亮或便签 | 批注列表 + 页面上的批注标记 | 6 种批注：高亮、下划线、删除线、便签、自由文本、矩形 | Six annotation types: highlight, underline, strikeout, sticky note, free text, rectangle |
| 4 | 页面管理 | 在缩略图栏多选页面 | 多选状态 + 操作按钮（旋转/删除/重排/提取） | 缩略图管理页面：旋转、删除、重排、提取为新文档 | Manage pages from thumbnails: rotate, delete, reorder, extract |
| 5 | 水印去除 | 打开「水印」，用自动检测或手动框选 | 检测结果列表/框选区域 | 添加水印，或自动检测重复水印对象后一键去除 | Add watermarks, or auto-detect repeating watermark objects and remove them |
| 6 | 深度编辑 | 双击正文文字进入编辑态 | 虚线编辑框 + 浮动工具胶囊 + 右侧面板 | 双击正文即改：遮盖原区域后按原样式重绘，字体缺失自动近似替换 | Double-click text to rewrite in place, redrawing with the original style |
| 7 | 密码与权限 | 打开「密码」（加密文档） | 加密状态 + 权限矩阵 + 导出明文副本入口 | 查看加密状态与权限矩阵，导出明文副本或内存去加密 | Inspect encryption status and permissions, export a plain copy or strip encryption |
| 8 | 导出图片 | 打开「导出」，选 DPI | DPI 选项 + 格式选择（PNG/JPG） | 页面导出 PNG/JPG，可选 DPI；图片也能反向合并成 PDF | Export pages to PNG/JPG at a chosen DPI; images can be merged back into PDF |

可选补位（替换/追加到 10 张内）：

| # | 场景 | 备注 |
|---|------|------|
| 9 | 表单字段填写 | 打开「表单」，列出 AcroForm 字段并填入新值（[FormPanel.tsx](../src/components/panels/FormPanel.tsx)） |
| 10 | 诊断面板 / 英文界面 | 诊断面板展示页数/大小/加密/扫描抽样/注释总数；切到英文界面（`Ctrl+Shift+L`）再拍一套 |

**OCR 面板不建议入镜**：`ocr` 为可选 feature，默认构建下调用返回 `ocr_unavailable`。要拍该场景必须先 `cargo build --features ocr`（需 Tesseract + 语言包）。

## 3. 商店文案

字段限制（来自 Partner Center 一览文档）：说明必填 ≤10,000 字符；简短说明上限 1000、最佳 <270；产品功能最多 20 条、每条 ≤200 字符；短标题 ≤50；排序标题／语音标题 ≤255；「此版本中的新增功能」≤1500（**首次提交留空**）。

### 3.1 简短说明

**中**（约 90 字）：

> 轻量、离线的 Windows PDF 阅读与编辑工具：阅读、批注、搜索、页面管理、水印增删、表单填写、密码保护与图片导出，全部在本机完成，文件不上传。

**EN**（约 260 字符，留出余量给 270 截断）：

> A lightweight, offline PDF reader and editor for Windows. Read, annotate, search, manage pages, add or remove watermarks, fill forms, protect with passwords, and export to images — all processed locally on your PC, with no file uploads.

### 3.2 说明（必填）

**中**：

```text
PDFe 是一款面向 Windows 的轻量 PDF 阅读与编辑工具，基于 PDFium 渲染，启动快、内存占用低，所有处理都在本机完成——文档不会上传到任何服务器。

阅读
· 连续滚动、单页、双页对开三种视图，缩放 10%–600%，支持适应宽度/页面
· 左侧缩略图导航与书签大纲，底部状态栏显示页码与文件名
· 深色 / 浅色主题，中 / 英 / 日三语界面

查找与批注
· 全文搜索，命中结果在页面上高亮并逐条跳转
· 6 种批注：高亮、下划线、删除线、便签、自由文本、矩形

页面与文档
· 旋转、删除、重排、提取页面；多文档合并；按页数、自定义范围或书签层级拆分
· 文本重写：双击正文进入编辑态，遮盖原区域后按原样式重绘（字体缺失自动近似替换并提示）
· 图片选中、移动、缩放、替换、删除

水印
· 添加文字/图片水印；按重复对象自动检测并去除，或手动框选区域去除

安全与导出
· 设置打开密码与权限密码，查看权限矩阵；持有密码时移除加密，或导出明文副本
· 导出页面为 PNG / JPG，可选 DPI；图片可反向合并为 PDF
· 表单（AcroForm）字段列出并填写后写回文档
· 文档诊断：页数、大小、是否加密、扫描版抽样、批注总数

快捷键
Ctrl+O 打开 · Ctrl+S 保存 · Ctrl+F 搜索 · Ctrl+Z/Ctrl+Y 撤销重做
Ctrl+1/2/3 视图切换 · Ctrl+Shift+L 语言 · Ctrl+Shift+T 主题 · F1 帮助

适用场景：日常阅读、合同/报告批注、扫描件整理、批量页面处理。

系统要求：Windows 10 1809 及以上（x64）。
```

**EN**：

```text
PDFe is a lightweight PDF reader and editor for Windows, built on PDFium for fast startup and low memory use. Everything runs locally on your PC — your documents are never uploaded.

READ
· Continuous, single-page and two-page views; zoom from 10% to 600%, fit-to-width and fit-to-page
· Thumbnail navigation and bookmark outline on the left; page number and file name in the status bar
· Dark and light themes; Chinese, English and Japanese interface

FIND & ANNOTATE
· Full-text search with on-page highlights and jump-to-hit
· Six annotation types: highlight, underline, strikeout, sticky note, free text, rectangle

PAGES & DOCUMENTS
· Rotate, delete, reorder and extract pages; merge documents; split by page count, custom ranges or bookmark levels
· Text rewrite: double-click text to edit in place, covering the original area and redrawing with the original style (falls back to a near match when a font is missing)
· Select, move, resize, replace and delete images

WATERMARKS
· Add text or image watermarks; auto-detect repeating watermark objects and remove them, or drag-select a region to remove

SECURITY & EXPORT
· Set open and permission passwords and review the permission matrix; remove encryption when you know the password, or export a plain copy
· Export pages to PNG / JPG at a chosen DPI; images can be merged back into PDF
· List and fill AcroForm fields, written back to the document on save
· Document diagnostics: page count, file size, encryption, scanned-page sampling, annotation total

SHORTCUTS
Ctrl+O open · Ctrl+S save · Ctrl+F search · Ctrl+Z / Ctrl+Y undo and redo
Ctrl+1/2/3 view modes · Ctrl+Shift+L language · Ctrl+Shift+T theme · F1 help

Good for everyday reading, marking up contracts and reports, cleaning up scanned files, and batch page work.

System requirements: Windows 10 version 1809 or later (x64).
```

### 3.3 产品功能（每条 ≤200 字符，不要自带项目符号）

**中（10 条）**

1. 连续 / 单页 / 双页三种阅读视图，10%–600% 缩放
2. 全文搜索，命中结果页面高亮并逐条跳转
3. 6 种批注：高亮、下划线、删除线、便签、自由文本、矩形
4. 页面旋转 / 删除 / 重排 / 提取，多文档合并，按页数或书签拆分
5. 双击正文直接改写文字，按原字体样式重绘
6. 水印添加；自动检测重复水印对象或手动框选去除
7. 打开密码与权限密码，权限矩阵查看，导出明文副本
8. 页面导出 PNG / JPG 并可指定 DPI，图片反向合并为 PDF
9. 表单（AcroForm）字段填写并写回文档
10. 全离线处理，文档不上传；中 / 英 / 日三语，深色与浅色主题

**EN（10 条）**

1. Continuous, single-page and two-page reading views with 10%–600% zoom
2. Full-text search with on-page highlights and jump-to-hit
3. Six annotation types: highlight, underline, strikeout, sticky note, free text, rectangle
4. Rotate, delete, reorder and extract pages; merge documents; split by pages or bookmarks
5. Double-click to rewrite text in place, redrawn with the original font style
6. Add watermarks; remove them by auto-detecting repeating objects or drag-selecting a region
7. Open and permission passwords, permission matrix, plain-copy export
8. Export pages to PNG / JPG at a chosen DPI; merge images back into PDF
9. Fill AcroForm fields and write them back to the document
10. Fully offline — no uploads; Chinese, English and Japanese UI; dark and light themes

### 3.4 补充字段

| 字段 | 上限 | 拟填值 |
|------|------|--------|
| 短标题 | 50 | `PDFe` |
| 排序标题 | 255 | `PDFe PDF reader editor` |
| 语音标题 | 255 | `PDFe` |
| 此版本中的新增功能 | 1500 | 首次提交**留空** |
| 其他系统要求 · 最低硬件 | ≤11 条 | `Windows 10 1809 或更高（x64）` / `64 位处理器` |
| 其他系统要求 · 推荐硬件 | ≤11 条 | `Windows 11` / `4 GB 内存或更多` |

## 4. Partner Center 侧待决策项（本文不代替决策）

- **类别**：建议「效率 / Productivity」，需在提交时确认可用类目
- **定价与市场**：免费 + 全市场（需确认是否只投 zh-CN / en-US 市场）
- **年龄分级**：按 Partner Center 问卷填写
- **隐私政策 URL**：本应用无网络通信、不上传文件；如提交问卷判定需要，需另行准备政策页面
- **身份替换**：清单 `Identity/@Name` 与 `Publisher` 换成 Partner Center 分配值，包由微软重签

## 5. 提交前检查清单

- [x] 300×300 一览图标 —— `docs/store-listing/StoreLogo-300x300.png`（勿用包内 50×50）
- [ ] ≥4 张（建议 8 张）≥1366×768 PNG，中英各一套，逐张核对像素
- [ ] 每张截图配 ≤200 字符说明，中英各一份
- [ ] 说明 / 简短说明 / 产品功能 中英双份文案落库
- [ ] 短标题、排序标题、系统要求填写
- [ ] 清单 Identity 替换 + 微软重签后重新走 `Add-AppxPackage` 侧载自测
- [ ] Windows App 认证工具包（WACK）本地预检通过
