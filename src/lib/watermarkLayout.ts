// 水印几何布局（前端镜像）
//
// 与 `src-tauri/src/watermark.rs` 中的 `estimate_text_width` / `anchor` / `tile_positions`
// 保持逐行一致，用于在画布上实时预览「水印会被放在哪里」。
// 改动任何一侧时，另一侧必须同步，并由 `tests/lib/watermarkLayout.test.ts`
// 与 Rust 单测（`watermark.rs` 的 `mod tests`）双向锁定。

/** 九宫格留白（pt），与后端 `MARGIN` 一致 */
export const WATERMARK_MARGIN = 36.0;

/** 平铺间距下限；低于该值时后端回退为 120pt */
export const DEFAULT_TILE_SPACING = 120.0;

export interface WatermarkAnchor {
  x: number;
  y: number;
}

/** 归一化位置因子（0–1，左下原点），与后端 `CustomPosition` 对应 */
export interface WatermarkFactors {
  x: number;
  y: number;
}

/** 与后端一致的最小字号（`opts.font_size.max(4.0)`） */
export const MIN_FONT_SIZE = 4.0;

/** 位置/平铺所需的最小样式字段（`WatermarkStyle` 的结构子集） */
export interface WatermarkLayoutStyle {
  position: string;
  tiled: boolean;
  tileSpacing: number;
  /** 画布拖放的归一化位置；非空且未平铺时优先于 position（与后端一致） */
  custom?: WatermarkFactors | null;
}

/**
 * 估算文本宽度：CJK 字符按全宽、其余按 0.55 倍字号。
 * 阈值 `0x2E80` 与后端一致（按 Unicode 码点判定）。
 */
export function estimateTextWidth(text: string, fontSize: number): number {
  let factor = 0;
  for (const ch of text) {
    factor += (ch.codePointAt(0) ?? 0) >= 0x2e80 ? 1.0 : 0.55;
  }
  return factor * fontSize;
}

/** 九宫格标识 → 归一化因子（左下原点，0–1） */
export function positionFactors(position: string): WatermarkFactors {
  let fx: number;
  let fy: number;
  switch (position) {
    case "top-left":
      fx = 0.0;
      fy = 1.0;
      break;
    case "top-center":
      fx = 0.5;
      fy = 1.0;
      break;
    case "top-right":
      fx = 1.0;
      fy = 1.0;
      break;
    case "middle-left":
      fx = 0.0;
      fy = 0.5;
      break;
    case "middle-right":
      fx = 1.0;
      fy = 0.5;
      break;
    case "bottom-left":
      fx = 0.0;
      fy = 0.0;
      break;
    case "bottom-center":
      fx = 0.5;
      fy = 0.0;
      break;
    case "bottom-right":
      fx = 1.0;
      fy = 0.0;
      break;
    default:
      fx = 0.5;
      fy = 0.5; // center
      break;
  }
  return { x: fx, y: fy };
}

/** 归一化因子 → 对象左下角坐标（PDF 点，左下原点） */
export function anchorFromFactors(
  fx: number,
  fy: number,
  pageWidth: number,
  pageHeight: number,
  objWidth: number,
  objHeight: number,
): WatermarkAnchor {
  const x = WATERMARK_MARGIN + fx * (pageWidth - 2.0 * WATERMARK_MARGIN - objWidth);
  const y = WATERMARK_MARGIN + fy * (pageHeight - 2.0 * WATERMARK_MARGIN - objHeight);
  return { x: Math.max(x, 0.0), y: Math.max(y, 0.0) };
}

/** 九宫格锚点 → 对象左下角坐标（PDF 点，左下原点） */
export function anchorPosition(
  position: string,
  pageWidth: number,
  pageHeight: number,
  objWidth: number,
  objHeight: number,
): WatermarkAnchor {
  const f = positionFactors(position);
  return anchorFromFactors(f.x, f.y, pageWidth, pageHeight, objWidth, objHeight);
}

/** 夹到 0–1 */
function clamp01(v: number): number {
  return Math.min(1.0, Math.max(0.0, v));
}

/**
 * 拖放反解：由对象在画布上的位置（CSS 像素，左上原点）反算归一化因子，
 * 使 `anchorFromFactors` 能还原出同一坐标（`anchorAt` 的逆）。
 *
 * `boxLeftPx` 为对象左边缘、`boxBottomPx` 为对象下边缘（CSS 像素）。
 */
export function factorsFromBox(
  boxLeftPx: number,
  boxBottomPx: number,
  scale: number,
  pageWidth: number,
  pageHeight: number,
  objWidth: number,
  objHeight: number,
): WatermarkFactors {
  if (scale <= 0) return { x: 0.5, y: 0.5 };
  // CSS（左上原点）→ PDF 点（左下原点）
  const pdfX = boxLeftPx / scale;
  const pdfY = pageHeight - boxBottomPx / scale;
  const denomX = pageWidth - 2.0 * WATERMARK_MARGIN - objWidth;
  const denomY = pageHeight - 2.0 * WATERMARK_MARGIN - objHeight;
  return {
    x: denomX > 0 ? clamp01((pdfX - WATERMARK_MARGIN) / denomX) : 0.5,
    y: denomY > 0 ? clamp01((pdfY - WATERMARK_MARGIN) / denomY) : 0.5,
  };
}

/** 平铺网格生成器：交错排列覆盖整页 */
export function tilePositions(
  pageWidth: number,
  pageHeight: number,
  objWidth: number,
  objHeight: number,
  spacing: number,
): WatermarkAnchor[] {
  const gap = spacing > 10.0 ? spacing : DEFAULT_TILE_SPACING;
  const stepX = objWidth + gap;
  const stepY = objHeight + gap;
  const out: WatermarkAnchor[] = [];
  let row = 0;
  let y = -stepY;
  while (y < pageHeight) {
    const offset = ((row % 2) * stepX) / 2.0;
    let x = -stepX + offset;
    while (x < pageWidth) {
      out.push({ x, y });
      x += stepX;
    }
    y += stepY;
    row += 1;
  }
  return out;
}

/**
 * 求某页上所有水印锚点（平铺或九宫格单点），与后端 `add_text_watermark` /
 * `add_image_watermark` 中的 `spots` 选择逻辑一致。
 */
export function watermarkAnchors(
  pageWidth: number,
  pageHeight: number,
  objWidth: number,
  objHeight: number,
  style: WatermarkLayoutStyle,
): WatermarkAnchor[] {
  if (style.tiled) {
    return tilePositions(pageWidth, pageHeight, objWidth, objHeight, style.tileSpacing);
  }
  // 平铺时忽略 custom（与后端一致）；其余情况 custom 优先，否则回退九宫格
  const custom = style.custom;
  if (custom) {
    return [
      anchorFromFactors(
        clamp01(custom.x),
        clamp01(custom.y),
        pageWidth,
        pageHeight,
        objWidth,
        objHeight,
      ),
    ];
  }
  return [anchorPosition(style.position, pageWidth, pageHeight, objWidth, objHeight)];
}
