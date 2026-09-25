import { test } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_TILE_SPACING,
  WATERMARK_MARGIN,
  anchorPosition,
  estimateTextWidth,
  tilePositions,
  watermarkAnchors,
} from "../../src/lib/watermarkLayout.ts";

// 用例与 `src-tauri/src/watermark.rs` 的 `mod tests` 对齐，
// 保证前端预览与后端落盘使用同一套几何规则。

function approx(a: number, b: number) {
  assert.ok(Math.abs(a - b) < 1.0, `approx ${a} vs ${b}`);
}

// ---------- estimateTextWidth ----------

test("estimateTextWidth：ASCII 按 0.55 倍字号", () => {
  approx(estimateTextWidth("Hello", 10.0), 27.5); // 5 * 0.55 * 10
});

test("estimateTextWidth：CJK 按全宽", () => {
  approx(estimateTextWidth("中你", 10.0), 20.0); // 2 * 1.0 * 10
});

test("estimateTextWidth：中英混排", () => {
  approx(estimateTextWidth("Hi中", 10.0), 21.0); // (2 * 0.55 + 1.0) * 10
});

test("estimateTextWidth：空串为 0", () => {
  assert.equal(estimateTextWidth("", 10.0), 0.0);
});

test("estimateTextWidth：阈值按码点判定（0x2E80）", () => {
  // U+2E7F 在阈值之下按窄字符，U+2E80 起按全宽
  approx(estimateTextWidth("\u2e7f", 10.0), 5.5);
  approx(estimateTextWidth("\u2e80", 10.0), 10.0);
  // 代理对（U+1F600）应作为一个码点，按全宽计
  approx(estimateTextWidth("\u{1f600}", 10.0), 10.0);
});

// ---------- anchorPosition ----------

test("anchorPosition：九宫格角点解析", () => {
  const pageW = 612.0;
  const pageH = 792.0;
  const objW = 100.0;
  const objH = 50.0;

  const tl = anchorPosition("top-left", pageW, pageH, objW, objH);
  approx(tl.x, WATERMARK_MARGIN);
  approx(tl.y, WATERMARK_MARGIN + (pageH - 2.0 * WATERMARK_MARGIN - objH));
  approx(tl.y, 706.0); // 36 + (792 - 72 - 50)

  const bl = anchorPosition("bottom-left", pageW, pageH, objW, objH);
  approx(bl.y, WATERMARK_MARGIN);
});

test("anchorPosition：未知位置回退 center", () => {
  const c = anchorPosition("center", 612.0, 792.0, 100.0, 50.0);
  const d = anchorPosition("not-a-position", 612.0, 792.0, 100.0, 50.0);
  approx(c.x, d.x);
  approx(c.y, d.y);
});

test("anchorPosition：极小页面下钳制为非负", () => {
  const p = anchorPosition("center", 10.0, 10.0, 100.0, 100.0);
  assert.ok(p.x >= 0.0, "x 应 >= 0");
  assert.ok(p.y >= 0.0, "y 应 >= 0");
});

// ---------- tilePositions ----------

test("tilePositions：间距低于阈值回退为 120", () => {
  const small = tilePositions(300.0, 300.0, 50.0, 50.0, 5.0);
  const fallback = tilePositions(300.0, 300.0, 50.0, 50.0, DEFAULT_TILE_SPACING);
  assert.equal(small.length, fallback.length);
});

test("tilePositions：超大对象仍返回有限坐标", () => {
  const pos = tilePositions(100.0, 100.0, 1000.0, 1000.0, 120.0);
  assert.ok(pos.length > 0, "从负向起点起步应至少生成一个位置");
  for (const p of pos) {
    assert.ok(Number.isFinite(p.x) && Number.isFinite(p.y));
  }
});

test("tilePositions：奇偶行交错半格", () => {
  const pageW = 300.0;
  const pageH = 1000.0;
  const objW = 50.0;
  const objH = 50.0;
  const spacing = 50.0;
  const pos = tilePositions(pageW, pageH, objW, objH, spacing);
  const stepX = objW + spacing; // 100
  const stepY = objH + spacing; // 100
  // 第 0 行从 y = -stepY 起、x = -stepX 起；第 1 行整体右移 stepX/2
  const row0 = pos.filter((p) => p.y === -stepY);
  const row1 = pos.filter((p) => p.y === -stepY + stepY);
  assert.ok(row0.length > 0 && row1.length > 0);
  assert.equal(row0[0].x, -stepX);
  assert.equal(row1[0].x - row0[0].x, stepX / 2);
});

// ---------- watermarkAnchors ----------

test("watermarkAnchors：非平铺返回单个锚点", () => {
  const spots = watermarkAnchors(612.0, 792.0, 100.0, 50.0, {
    position: "center",
    tiled: false,
    tileSpacing: 120.0,
  });
  assert.equal(spots.length, 1);
});

test("watermarkAnchors：平铺返回多点且与 tilePositions 一致", () => {
  const args = [300.0, 300.0, 50.0, 50.0] as const;
  const style = { position: "center", tiled: true, tileSpacing: 120.0 };
  const viaHelper = watermarkAnchors(args[0], args[1], args[2], args[3], style);
  const viaTile = tilePositions(args[0], args[1], args[2], args[3], style.tileSpacing);
  assert.deepEqual(viaHelper, viaTile);
});
