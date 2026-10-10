import { describe, expect, it } from "vitest";

import {
  JIMENG_POOL_CONSOLE_URL,
  JIMENG_POOL_PING_URL,
  JIMENG_POOL_PORT,
} from "./jimengPoolConsole";

describe("即梦账号池控制台地址", () => {
  it("指向根路径——控制台 HTML（含 setup/login 表单）由服务端在 / 提供", () => {
    expect(JIMENG_POOL_CONSOLE_URL).toBe(`http://127.0.0.1:${JIMENG_POOL_PORT}/`);
  });

  it('不是 /account-pool/——那是 API 前缀，未登录返回 {"error":"未登录"}，页面会卡死在那句 JSON', () => {
    // 2026-10-10 的实际缺陷：窗口指向 /account-pool/，用户永远到不了登录表单。
    expect(JIMENG_POOL_CONSOLE_URL).not.toContain("/account-pool");
  });

  it("探测地址是 /ping（代理未运行时用于回退到系统浏览器）", () => {
    expect(JIMENG_POOL_PING_URL).toBe(`http://127.0.0.1:${JIMENG_POOL_PORT}/ping`);
  });
});
