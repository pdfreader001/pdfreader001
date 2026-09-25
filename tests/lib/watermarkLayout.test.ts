import { test } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_TILE_SPACING,
  WATERMARK_MARGIN,
  anchorFromFactors,
  anchorPosition,
  estimateTextWidth,
  factorsFromBox,
  positionFactors,
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

// ---------- positionFactors / anchorFromFactors ----------

test("positionFactors：九宫格因子映射", () => {
  assert.deepEqual(positionFactors("top-left"), { x: 0.0, y: 1.0 });
  assert.deepEqual(positionFactors("middle-right"), { x: 1.0, y: 0.5 });
  assert.deepEqual(positionFactors("bottom-center"), { x: 0.5, y: 0.0 });
});

test("positionFactors：center 与未知位置同因子", () => {
  assert.deepEqual(positionFactors("center"), { x: 0.5, y: 0.5 });
  assert.deepEqual(positionFactors("not-a-position"), positionFactors("center"));
});

test("anchorFromFactors：与 anchorPosition 等价", () => {
  const args = [612.0, 792.0, 100.0, 50.0] as const;
  for (const position of ["top-left", "center", "bottom-right", "middle-left"]) {
    const f = positionFactors(position);
    const viaFactors = anchorFromFactors(f.x, f.y, args[0], args[1], args[2], args[3]);
    const viaPosition = anchorPosition(position, args[0], args[1], args[2], args[3]);
    approx(viaFactors.x, viaPosition.x);
    approx(viaFactors.y, viaPosition.y);
  }
});

// ---------- watermarkAnchors：custom 定位 ----------

test("watermarkAnchors：custom 优先于 position（非平铺）", () => {
  const args = [612.0, 792.0, 100.0, 50.0] as const;
  const custom = { x: 0.25, y: 0.75 };
  const style = { position: "top-left", tiled: false, tileSpacing: 120.0, custom };
  const spots = watermarkAnchors(args[0], args[1], args[2], args[3], style);
  assert.equal(spots.length, 1);
  const expected = anchorFromFactors(custom.x, custom.y, args[0], args[1], args[2], args[3]);
  approx(spots[0].x, expected.x);
  approx(spots[0].y, expected.y);
});

test("watermarkAnchors：平铺时忽略 custom", () => {
  const args = [300.0, 300.0, 50.0, 50.0] as const;
  const base = { position: "center", tiled: true, tileSpacing: 120.0 };
  const withCustom = { ...base, custom: { x: 0.1, y: 0.9 } };
  const a = watermarkAnchors(args[0], args[1], args[2], args[3], base);
  const b = watermarkAnchors(args[0], args[1], args[2], args[3], withCustom);
  assert.deepEqual(a, b);
});

test("watermarkAnchors：custom 越界钳制到 0–1", () => {
  const args = [612.0, 792.0, 100.0, 50.0] as const;
  const style = {
    position: "center",
    tiled: false,
    tileSpacing: 120.0,
    custom: { x: 2.0, y: -1.0 },
  };
  const spots = watermarkAnchors(args[0], args[1], args[2], args[3], style);
  const expected = anchorFromFactors(1.0, 0.0, args[0], args[1], args[2], args[3]);
  approx(spots[0].x, expected.x);
  approx(spots[0].y, expected.y);
});

// ---------- factorsFromBox ----------

test("factorsFromBox：与 anchorFromFactors 往返一致", () => {
  const pageW = 612.0;
  const pageH = 792.0;
  const objW = 100.0;
  const objH = 50.0;
  const scale = 1.5;
  const f = { x: 0.3, y: 0.7 };
  const a = anchorFromFactors(f.x, f.y, pageW, pageH, objW, objH);
  // PDF 左下原点 → CSS 左上原点：left = x * scale，bottom = (pageH - y) * scale
  const back = factorsFromBox(a.x * scale, (pageH - a.y) * scale, scale, pageW, pageH, objW, objH);
  approx(back.x, f.x);
  approx(back.y, f.y);
});

test("factorsFromBox：极值钳制", () => {
  const pageW = 612.0;
  const pageH = 792.0;
  const objW = 100.0;
  const objH = 50.0;
  const far = factorsFromBox(99999.0, -99999.0, 1.0, pageW, pageH, objW, objH);
  assert.equal(far.x, 1.0);
  assert.equal(far.y, 1.0);
  const near = factorsFromBox(-99999.0, 99999.0, 1.0, pageW, pageH, objW, objH);
  assert.equal(near.x, 0.0);
  assert.equal(near.y, 0.0);
});

test("factorsFromBox：scale<=0 回退中心", () => {
  assert.deepEqual(factorsFromBox(10.0, 10.0, 0.0, 612.0, 792.0, 100.0, 50.0), {
    x: 0.5,
    y: 0.5,
  });
});
