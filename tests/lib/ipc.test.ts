import { test } from "node:test";
import assert from "node:assert/strict";
import { isApiError } from "../../src/lib/ipc.ts";

test("isApiError 识别标准 ApiError 结构", () => {
  assert.equal(isApiError({ code: "password", message: "需要密码" }), true);
  assert.equal(isApiError({ code: "io", message: "磁盘满", args: { path: "/tmp" } }), true);
});

test("isApiError 拒绝缺少字段的对象", () => {
  assert.equal(isApiError({ code: "x" }), false, "缺 message");
  assert.equal(isApiError({ message: "x" }), false, "缺 code");
  assert.equal(isApiError({ code: 123, message: "x" }), false, "code 必须是 string");
  assert.equal(isApiError({ code: "x", message: 456 }), false, "message 必须是 string");
});

test("isApiError 拒绝非对象", () => {
  assert.equal(isApiError(null), false);
  assert.equal(isApiError(undefined), false);
  assert.equal(isApiError("error"), false);
  assert.equal(isApiError(42), false);
  assert.equal(isApiError(new Error("oops")), false, "原生 Error 不是 ApiError");
});

test("isApiError 接受 args 可选字段", () => {
  const noArgs = { code: "x", message: "y" };
  const withArgs = { code: "x", message: "y", args: { detail: "z" } };
  const nullArgs = { code: "x", message: "y", args: null };
  assert.equal(isApiError(noArgs), true);
  assert.equal(isApiError(withArgs), true);
  // args 类型不重要——即使是 null 也应该被接受（可选字段）
  assert.equal(isApiError(nullArgs), true);
});
