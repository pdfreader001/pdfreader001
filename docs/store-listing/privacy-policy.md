# PDFe 隐私政策 · PDFe Privacy Policy

适用产品：**PDFe**（Windows 桌面应用）　应用标识：`com.konnyyuan.pdfe`　版本：`0.1.0`
生效日期：2026-09-26　最近更新：2026-09-26

> 本文档提供中文与 English 两个版本，两部分内容一致；如有歧义，以中文版为准。
> This document is provided in Chinese and English with identical content; the Chinese version prevails in case of ambiguity.

---

## 一、中文版

### 1. 摘要

**PDFe 是一款完全在你本机（本地）运行的 Windows 桌面 PDF 工具。它不包含任何联网功能，不收集、不上传、不共享你的个人信息，也不会把你的文档内容发送到任何服务器。**

你的 PDF 文件、图片与你在应用内填写的文本，全部只在你自己的电脑上被读取、处理和保存。

### 2. 我们收集哪些信息

**不收集任何信息。** 具体而言：

- 不收集个人身份信息（姓名、邮箱、电话、地址等），也不要求注册或登录账号；
- 不收集设备标识、使用行为、崩溃日志或统计数据，应用内**不含**任何遥测、分析（analytics）、广告或埋点 SDK；
- 不收集你的文档内容、文件名、批注文字或表单填写内容；
- 不进行任何形式的数据共享、出售或转让给第三方（因为不存在可共享的数据）。

### 3. 你的文档如何处理

以下操作**全部在本机完成**，不产生任何网络传输：

| 功能 | 处理位置 |
| --- | --- |
| 打开、渲染、翻页、缩放、搜索 | 本机内存（PDFium 引擎） |
| 批注、页面管理（旋转/复制/删除/插入/提取） | 本机内存 + 你指定的输出文件 |
| 水印添加与水印检测/去除 | 本机内存 |
| 深度编辑（原位改字） | 本机内存 |
| 合并、拆分、表单填写 | 本机内存 + 你指定的输出文件 |
| 加密状态查看、导出明文副本、导出加密副本 | 本机内存 + 你指定的输出文件 |
| 导出图片（PNG/JPEG） | 本机内存 + 你指定的输出文件夹 |
| OCR 文字识别（可选构建，默认未启用） | 本机内存（Tesseract 引擎，离线运行） |

### 4. 仅保存在你本机的数据

应用会在你自己的电脑上保存两项界面设置，用于改善下次使用体验：

- **阅读位置记忆**：键名形如 `pdfe:pos:{文件名}:{页数}`，内容为 `{ 页码, 缩放比例 }`；
- **界面语言**：键名 `pdfe:locale`，内容为 `zh` / `en` / `ja` 之一。

这两项数据保存在应用的本地 WebView 存储区中，**不会离开你的设备**，且卸载应用后即被移除。除此之外，应用不保存「最近打开文件」列表、不保存文档内容副本、不建立索引。

### 5. 应用会写入哪些文件

应用只在**你主动发起保存或导出**时写文件，全部写到你通过系统「打开/保存」对话框明确指定的位置：

- **保存文档**：采用原子写入——先在目标文件所在目录生成一个同名 `.tmp` 临时文件，写入成功后再替换目标文件。若过程中断，最多留下一个你目标目录下的 `.tmp` 文件，可自行删除；
- **导出**：导出明文副本、加密副本、合并结果、拆分结果、表单结果、图片等，均写到你在对话框中选定的路径或文件夹。

应用不会向系统临时目录、注册表或任何隐藏位置写入你的文档数据。

### 6. 应用申请的权限

PDFe 以 Windows 全信任桌面程序（full-trust Win32）形式打包，仅声明 `runFullTrust` 一项能力。它**不声明**摄像头、麦克风、位置、通讯录、日历、短信等任何 Windows 受限功能权限。应用访问文件的方式是：你在系统文件选择对话框中明确选中某个文件或文件夹后，应用才读写该位置。

### 7. 第三方组件

应用内置以下开源组件，它们**均在本机离线运行、不进行网络通信、不收集数据**：

- **PDFium** —— PDF 渲染与解析引擎；
- **Tesseract**（可选）—— OCR 文字识别引擎，仅在你自行以 `ocr` 特性构建的版本中启用，默认发布版本未启用；
- **Tauri / Microsoft Edge WebView2** —— 应用界面运行时。WebView2 是 Windows 的系统组件，其自身的数据处理行为由 Microsoft 的条款约束，与应用无关。

### 8. 儿童隐私

应用不收集任何人的信息（包括儿童的信息），不包含广告，也不含面向儿童的定向内容。因此不存在与儿童数据相关的收集或使用行为。

### 9. 数据的删除

- 想清除本机保存的界面设置：卸载 PDFe 即可（或清除应用本地存储）；
- 想删除你导出的文件：在文件管理器中自行删除即可；
- 应用侧没有服务器副本，因此**无需**向我们申请删除任何数据。

### 10. 本政策的变更

若未来版本引入需要联网的功能，我们会在发布前更新本政策，并在本页顶部标注「最近更新」日期。请以本页当前版本为准。

### 11. 联系我们

对本政策有疑问，请通过 GitHub Issues 联系：<https://github.com/pdfreader001/pdfreader001/issues>

---

## PDFe Privacy Policy (English)

Product: **PDFe** (Windows desktop application)　App identity: `com.konnyyuan.pdfe`　Version: `0.1.0`
Effective date: 2026-09-26　Last updated: 2026-09-26

### 1. Summary

**PDFe is a Windows desktop PDF tool that runs entirely on your own device. It has no networking capabilities and does not collect, upload, or share your personal information or your document contents.**

Your PDF files, images, and any text you type in the app are read, processed, and saved only on your own computer.

### 2. Information We Collect

**None.** Specifically:

- We do not collect personal identifiers (name, email, phone, address), and there is no account, sign-up, or sign-in;
- We do not collect device identifiers, usage behavior, crash logs, or statistics. The app contains **no** telemetry, analytics, advertising, or tracking SDKs;
- We do not collect your document contents, file names, annotation text, or form-field values;
- We do not share, sell, or transfer data to third parties — there is no such data to share.

### 3. How Your Documents Are Processed

Every one of the following operations **happens locally on your device**, with no network transmission:

| Feature | Where it runs |
| --- | --- |
| Open, render, page through, zoom, search | Local memory (PDFium) |
| Annotate; manage pages (rotate / copy / delete / insert / extract) | Local memory + the output file you choose |
| Add watermarks; detect and remove watermarks | Local memory |
| Deep edit (rewrite text in place) | Local memory |
| Merge, split, fill forms | Local memory + the output file you choose |
| Inspect encryption, export a plain copy, export an encrypted copy | Local memory + the output file you choose |
| Export images (PNG/JPEG) | Local memory + the folder you choose |
| OCR (optional build, off by default) | Local memory (Tesseract, fully offline) |

### 4. Data Stored on Your Device Only

The app stores two UI preferences on your own computer to improve your next session:

- **Reading position**: key of the form `pdfe:pos:{file name}:{page count}`, holding `{ page, zoom }`;
- **Interface language**: key `pdfe:locale`, holding one of `zh` / `en` / `ja`.

Both live in the app's local WebView storage, **never leave your device**, and are removed when you uninstall the app. Beyond these, the app keeps no "recent files" list, no copy of your documents, and no index.

### 5. Files the App Writes

The app writes files only when **you** initiate a save or export, and always to the location you explicitly choose in the system open/save dialog:

- **Saving a document** uses an atomic write: a sibling `<name>.tmp` file is created in the target file's own directory, then renamed over the target on success. If interrupted, at most a `.tmp` file remains in your target directory and you may delete it;
- **Exporting** (plain copy, encrypted copy, merge result, split result, form result, images, etc.) writes to the path or folder you picked in the dialog.

The app never writes your document data to the system temp directory, the registry, or any hidden location.

### 6. Permissions the App Requests

PDFe ships as a Windows full-trust desktop app (full-trust Win32) and declares exactly one capability: `runFullTrust`. It **declares no** restricted Windows capabilities — no camera, microphone, location, contacts, calendar, or messaging. The app accesses files only where you have explicitly selected a file or folder in the system file dialog.

### 7. Third-Party Components

The app bundles the following open-source components. **All of them run locally and offline; none performs network communication or collects data:**

- **PDFium** — PDF rendering and parsing engine;
- **Tesseract** (optional) — OCR engine, enabled only in builds you compile yourself with the `ocr` feature; it is off in the default release build;
- **Tauri / Microsoft Edge WebView2** — the UI runtime. WebView2 is a Windows system component; its own data practices are governed by Microsoft's terms and are unrelated to this app.

### 8. Children's Privacy

The app collects information from no one, including children. It contains no advertising and no child-directed content. Accordingly, there is no collection or use of children's data.

### 9. Deleting Your Data

- To clear locally stored UI preferences: uninstall PDFe (or clear the app's local storage);
- To delete files you exported: delete them yourself in File Explorer;
- There is no server-side copy, so **no** deletion request to us is needed.

### 10. Changes to This Policy

If a future version introduces features that require network access, we will update this policy before release and revise the "Last updated" date at the top of this page. The version currently published here governs.

### 11. Contact

Questions about this policy: please open an issue at <https://github.com/pdfreader001/pdfreader001/issues>

---

## 附：内部备注（发布前请处理，不必随页面公开）

**本文每条断言的代码依据**（便于后续版本核验，一旦引入联网功能须同步修改本文）：

| 断言 | 依据 |
| --- | --- |
| 无联网功能 | `src/` 内无 `fetch` / `XMLHttpRequest` / `WebSocket` / axios 调用；[Cargo.toml](../src-tauri/Cargo.toml) 与 [package.json](../package.json) 中无 `tauri-plugin-http` / `tauri-plugin-updater` / 任何分析或广告依赖 |
| 仅两个本机设置项 | [store.ts](../src/state/store.ts#L275-L294)（`POS_PREFIX`）、#L353 / #L533（`pdfe:locale`） |
| 原子写入 `.tmp` | [document.rs](../src-tauri/src/document.rs#L251-L267) |
| 仅 3 处文件写入，全为用户主动发起 | [convert.rs](../src-tauri/src/convert.rs#L278)（导出）、[security.rs](../src-tauri/src/security.rs#L161)（明文副本）、[security.rs](../src-tauri/src/security.rs#L274)（加密副本） |
| 文件选择只走系统对话框 | 8 处 `plugin-dialog` 的 `open` / `save`（App.tsx、ConvertPanel、ThumbnailPanel、EditPanel、MergePanel、SecurityPanel、SplitPanel、WatermarkPanel） |
| 只声明 `runFullTrust` | [AppxManifest.xml](../src-tauri/msix/AppxManifest.xml#L61-L64) |
| 默认构建无 OCR | [Cargo.toml](../src-tauri/Cargo.toml#L40-L43)（`default = []`，`ocr` 为可选特性） |

**发布前必须补的两项**：

1. **托管 URL**：本文件是 Markdown，Partner Center 要求的是**公开可访问的网页 URL**。具体操作见下方「附：托管到 GitHub Pages 步骤」，并把最终 URL 填进 Partner Center 的「隐私政策 URL」字段。
2. **联系方式确认**：正文 §11 目前用 GitHub Issues 作为联系方式。如果你希望改用支持邮箱（Partner Center 也会要求一个支持联系方式），请替换该段的两处链接。

**与商店问卷的对应关系**：[store-listing-materials.md](store-listing-materials.md) §4 提到「如提交问卷判定需要，需另行准备政策页面」——本文件即为该页面草稿。

---

## 附：托管到 GitHub Pages 步骤

仓库现状（2026-09-26 实测）：远端 `https://github.com/pdfreader001/pdfreader001.git`，默认分支 `main`，仓库内**无** `.github/workflows/`，本机**未安装** `gh` CLI（故以下以网页操作为主）。

### 第 0 步 · 先确认仓库可见性（决定能走哪条路线）

**GitHub Free 计划下，只有公开仓库能启用 Pages。** 打开 <https://github.com/pdfreader001/pdfreader001>，若**退出登录也能看到仓库内容**即为公开。私有仓库需 GitHub Pro / Team / Enterprise，或改用路线 B 之外的方案。

### 路线 A · 零新增文件，最快（仓库为公开时用）

1. 网页打开仓库 → 右上 `Settings` → 左侧栏 `Pages`。
2. `Build and deployment` → `Source` 选 **`Deploy from a branch`**。
3. `Branch` 选 **`main`**，右侧目录选 **`/docs`** → `Save`。
4. 等 1–2 分钟，页面顶部出现 `Your site is live at https://pdfreader001.github.io/pdfreader001/`。
5. 政策页最终 URL 为：
   `https://pdfreader001.github.io/pdfreader001/store-listing/privacy-policy.html`
   （Jekyll 会把 `.md` 渲染成 `.html`，**后缀必须写 `.html`**。）

**路线 A 的三个副作用，务必先确认可接受：**

- **整个 `docs/` 都会被公开**：含 `superpowers/plans/2026-09-18-implementation-plan.md`、`superpowers/specs/2026-09-18-pdf-editor-design.md`、`store-listing-materials.md`（内部素材清单）与 `samples/*.pdf`（约 1 MB）。若仓库本就公开，这些内容已在 GitHub 上可见，Pages 不增加暴露面；但**若你计划把私有仓库转为公开**，需先决定这批内部文档能否公开。
- **本文件的「内部备注」段会被一起发布**，且其中的 `../Cargo.toml`、`../package.json` 等相对链接在 Pages 上全部 404（站点根是 `docs/`，不是仓库根）。**发布前建议把「内部备注」整段删掉或移到另一个文件。**
- 未加 `_config.yml` 时页面是**无样式纯 HTML**。想要基本排版，可在 `docs/` 下新增 `_config.yml`，内容一行：`theme: minima`（GitHub Pages 内置支持，无需 Gemfile）。另建议加 `docs/index.md` 作落地页，否则站点根路径 404。

### 路线 B · 只发布隐私政策单页（推荐，避免内部文档外泄）

1. 新建目录 `site/`（仓库根），放入渲染好的 `index.html`——内容为本文件正文（中英两版），**不含「内部备注」段**。
2. 新建 `.github/workflows/pages.yml`，内容：

   ```yaml
   name: Deploy privacy policy to GitHub Pages

   on:
     push:
       branches: [main]
       paths:
         - "site/**"
         - ".github/workflows/pages.yml"
     workflow_dispatch:

   permissions:
     contents: read
     pages: write
     id-token: write

   concurrency:
     group: pages
     cancel-in-progress: true

   jobs:
     deploy:
       runs-on: ubuntu-latest
       environment:
         name: github-pages
         url: ${{ steps.deploy.outputs.page_url }}
       steps:
         - uses: actions/checkout@v4
         - uses: actions/configure-pages@v5
         - uses: actions/upload-pages-artifact@v3
           with:
             path: site
         - id: deploy
           uses: actions/deploy-pages@v4
   ```

3. 仓库 `Settings` → `Pages` → `Source` 选 **`GitHub Actions`**（注意：与路线 A 的选项不同）。
4. 提交并推送到 `main`；Actions 自动构建部署。
5. 最终 URL：`https://pdfreader001.github.io/pdfreader001/`

**约束**：私有仓库用 Actions 发布 Pages **同样**需要 Pro / Team / Enterprise；Free 私有仓库两条路线都走不通。
**第三条路**（既不公开 `docs/`、也不是公开仓库时）：把政策页单独放进一个 `gh-pages` 分支，`Settings → Pages → Source` 选该分支的 `/ (root)`。

### 第 N 步 · 验证（两条路线通用，必做）

1. 用**无痕窗口**（未登录 GitHub）打开最终 URL —— Partner Center 审核以匿名身份检查，登录才可见的页面会被判不合格。
2. 确认浏览器**渲染出页面**而不是触发下载（这正是不能把 `.md` 直链填进 Partner Center 的原因）。
3. 换手机 / 另一台设备再开一次，确认公网可达。
4. 若 Chrome 报 `404`：先确认文件名后缀写的是 `.html` 而非 `.md`，再确认 Pages 部署已从 `Queued` 变为 `Active`。

### 第 N+1 步 · 回填

1. Partner Center → 应用 → 「隐私政策 URL」填上述最终 URL（**只能是 `.html` 或站点根，不能是 `.md`**）。
2. 更新 [store-listing-materials.md](store-listing-materials.md) §4 的「隐私政策 URL」一行，并在 §5 检查清单补一项「隐私政策已托管并回填」。
3. **URL 一旦提交建议长期保持稳定**：项目站路径由 `owner/repo` 决定，改仓库名或换账号都会导致 URL 失效、需重新填写，故优先用项目站固定路径而非临时托管。

