import { test } from "node:test";
import assert from "node:assert/strict";
import { LruCache, pageKey, thumbKey } from "../../src/lib/bitmapCache.ts";

// ---------- LruCache ----------

test("LruCache 基本 put/get", () => {
  const c = new LruCache<string, number>(100, (_, v) => v);
  c.set("a", 30);
  c.set("b", 40);
  assert.equal(c.get("a"), 30);
  assert.equal(c.get("b"), 40);
  assert.equal(c.get("c"), undefined);
});

test("LruCache 触碰更新 LRU 顺序", () => {
  const c = new LruCache<string, number>(100, (_, v) => v);
  c.set("a", 30);
  c.set("b", 30);
  c.set("c", 30); // 30+30+30=90 ≤ 100, 都在
  c.get("a");      // 触碰 a → 移到末尾，淘汰顺序：b → c → a
  c.set("d", 30);  // 90+30=120 > 100 → 淘汰 b（最老的）
  assert.equal(c.get("b"), undefined, "b 应被淘汰");
  assert.equal(c.get("a"), 30, "a 被触碰过不应淘汰");
  assert.equal(c.get("c"), 30);
  assert.equal(c.get("d"), 30);
});

test("LruCache 超预算逐步淘汰", () => {
  const c = new LruCache<string, number>(50, (_, v) => v);
  c.set("a", 20);
  c.set("b", 20);
  c.set("c", 20); // 40+20=60 > 50 → 淘汰 a; 20+20=40 ≤ 50
  assert.equal(c.get("a"), undefined);
  assert.equal(c.get("b"), 20);
  assert.equal(c.get("c"), 20);
});

test("LruCache 同 key set 覆盖旧值并更新预算", () => {
  const c = new LruCache<string, number>(50, (_, v) => v);
  c.set("a", 40);
  c.set("a", 10); // 同 key，先减 40 再加 10 → 净 10
  c.set("b", 45); // 10+45=55 > 50 → 淘汰 a
  assert.equal(c.get("a"), undefined);
  assert.equal(c.get("b"), 45);
});

test("LruCache delete 释放预算", () => {
  const c = new LruCache<string, number>(50, (_, v) => v);
  c.set("a", 30);
  c.set("b", 30); // 60 > 50 → 淘汰 a
  assert.equal(c.get("a"), undefined);
  assert.equal(c.get("b"), 30);
  c.delete("b");
  c.set("c", 40); // 释放后预算 50 可用
  assert.equal(c.get("c"), 40);
});

test("LruCache clear 清空全部", () => {
  const c = new LruCache<string, number>(100, (_, v) => v);
  c.set("a", 30);
  c.set("b", 40);
  c.clear();
  assert.equal(c.get("a"), undefined);
  assert.equal(c.get("b"), undefined);
});

test("LruCache 零预算立即淘汰", () => {
  const c = new LruCache<string, number>(0, (_, v) => v);
  c.set("a", 100);
  assert.equal(c.get("a"), undefined, "预算 0 时 set 后应立即淘汰");
});

test("LruCache sizeOf 正确计入 key", () => {
  const c = new LruCache<string, number>(10, (k, v) => k.length + v);
  c.set("abc", 5); // 3+5=8
  c.set("ab", 3);  // 2+3=5 → 8+5=13 > 10 → 淘汰 abc; 剩 5
  assert.equal(c.get("abc"), undefined);
  assert.equal(c.get("ab"), 3);
});

// ---------- key helpers ----------

test("pageKey 格式正确", () => {
  assert.equal(pageKey(1, 5, 2.0), "1:5:2.000");
  assert.equal(pageKey(99, 0, 1.5), "99:0:1.500");
  assert.equal(pageKey(0, 100, 0.5), "0:100:0.500");
});

test("thumbKey 格式正确", () => {
  assert.equal(thumbKey(1, 5), "1:t:5");
  assert.equal(thumbKey(99, 0), "99:t:0");
});

test("pageKey 相同参数产生相同 key（可用于缓存命中）", () => {
  const a = pageKey(42, 7, 1.333);
  const b = pageKey(42, 7, 1.333);
  assert.equal(a, b);
});
