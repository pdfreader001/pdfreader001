# PDFe

桌面 PDF 编辑器（Windows），基于 **Tauri 2 + React + TypeScript + Rust + PDFium**。

## 状态

**功能完整度（参考 [docs/superpowers/plans/2026-09-18-implementation-plan.md](docs/superpowers/plans/2026-09-18-implementation-plan.md)）**：

- ✅ **M0** 环境与骨架
- ✅ **M1** 文档核心管线（PDFium 集成、打开/保存、撤销栈）
- ✅ **M2** 阅读器 UI（三栏布局、缩略图、搜索高亮、主题）
- ✅ **M3** 页面管理 + 合并拆分（旋转/删除/重排/提取、合并向导、拆分）
- ✅ **M4** 水印（添加/去除 - 自动检测 + 手动框选）
- ✅ **M5** 深度编辑（双击文字重写、新增文本框、图片替换、扫描版检测）
- ✅ **M6** 安全 + 导出（明文副本、内存去加密、PNG/JPEG 导出含 DPI）
- ✅ **M7** 打磨（性能优化、错误处理 review、性能基线）
- ⏸️ 不上架、不打 MSIX 包（按约束）

## 测试

| 类型 | 数量 | 位置 |
|------|------|------|
| Rust 单元/集成 | **127** | `src-tauri/tests/` |
| TypeScript 类型检查 | 0 错误 | `npx tsc --noEmit` |
| 性能基线（2/100/500 页） | 13 项 | `src-tauri/tests/perf.rs` |

运行：
```bash
# 后端
cd src-tauri
cargo test -- --test-threads=1    # 首次会生成 large_100/500.pdf fixture

# 前端
npx tsc --noEmit
```

## 架构

```
src/                      # 前端（React + TypeScript）
  components/
    Canvas.tsx           # 画布 + PageView（精细订阅 + React.memo）
    Toolbar.tsx          # 顶部工具栏
    SearchPanel.tsx       # 搜索面板
    TaskPanel.tsx         # 所有任务面板（合并/拆分/水印/编辑/安全/导出）
    ThumbnailPanel.tsx   # 缩略图（虚拟滚动 + useCallback + close 释放显存）
  state/store.ts          # zustand 单一数据源
  lib/
    ipc.ts                # Tauri invoke 封装
    bitmapCache.ts        # LRU + ImageBitmap.close() 显存释放
  i18n.ts                 # 中英文 + translateError

src-tauri/src/
  render.rs               # 渲染 + 文本提取 + 搜索 + pick_text
  edit_ext.rs             # 深度编辑（M5，纯函数 + Tauri command）
  watermark_remove.rs     # 水印去除（M4）
  pages.rs                # 页面操作（M3）
  watermark.rs            # 加水印（M4）
  document.rs             # 文档打开/关闭/撤销/重做
  security.rs             # 安全状态 + 明文副本（M6）
  convert.rs              # PNG/JPEG 导出 + 图片合并为 PDF（M6）
  error.rs                # AppError 43 个 code + i18n code 一致性
  lib.rs                  # 命令注册入口
```

## 开发约定

- 所有后端命令走 `AppError` + 前端 `translateError()` 本地化
- 大操作前 `push_snapshot()`，前端 `setUndoRedo(refreshUndoRedo(id))`
- 选区坐标：CSS 像素（左上原点）/ PDF 点（左下原点），转换公式见 `.trae/skills/pdfe-milestone-delivery/SKILL.md`
- 提交规范：`feat|fix|docs|test|chore(scope): 描述`

## 推荐 IDE 配置

- VS Code + Tauri 扩展 + rust-analyzer