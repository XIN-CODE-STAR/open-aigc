import { describe, expect, it } from "vitest";

import { formatDateTime, formatRelativeTime } from "./time";

/** 固定「现在」，让断言不随时间漂移。 */
const NOW = new Date("2026-10-14T12:00:00.000Z");

/** 相对 NOW 偏移若干毫秒，返回 ISO 字符串。 */
function ago(ms: number): string {
  return new Date(NOW.getTime() - ms).toISOString();
}

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

describe("formatRelativeTime", () => {
  it("一分钟内显示「刚刚」", () => {
    expect(formatRelativeTime(ago(0), NOW)).toBe("刚刚");
    expect(formatRelativeTime(ago(30 * 1000), NOW)).toBe("刚刚");
    expect(formatRelativeTime(ago(MINUTE - 1), NOW)).toBe("刚刚");
  });

  it("一小时内显示分钟", () => {
    expect(formatRelativeTime(ago(MINUTE), NOW)).toBe("1分钟前");
    expect(formatRelativeTime(ago(59 * MINUTE), NOW)).toBe("59分钟前");
    expect(formatRelativeTime(ago(HOUR - 1), NOW)).toBe("59分钟前");
  });

  it("刚好一小时按小时计，不残留「60分钟前」", () => {
    expect(formatRelativeTime(ago(HOUR), NOW)).toBe("1小时前");
    expect(formatRelativeTime(ago(90 * MINUTE), NOW)).toBe("1小时前");
  });

  it("一天内显示小时", () => {
    expect(formatRelativeTime(ago(HOUR), NOW)).toBe("1小时前");
    expect(formatRelativeTime(ago(23 * HOUR), NOW)).toBe("23小时前");
    expect(formatRelativeTime(ago(DAY - 1), NOW)).toBe("23小时前");
  });

  it("30 天内显示天", () => {
    expect(formatRelativeTime(ago(DAY), NOW)).toBe("1天前");
    expect(formatRelativeTime(ago(3 * DAY), NOW)).toBe("3天前");
    expect(formatRelativeTime(ago(29 * DAY), NOW)).toBe("29天前");
  });

  it("同一年超过 30 天显示月日", () => {
    // 2026-10-14 往前 45 天 → 2026-08-30
    expect(formatRelativeTime(ago(45 * DAY), NOW)).toBe("8月30日");
  });

  it("跨年显示完整年月日", () => {
    // 2026-10-14 往前 300 天 → 2025-12-18
    expect(formatRelativeTime(ago(300 * DAY), NOW)).toBe("2025年12月18日");
  });

  it("时钟偏移（时间戳略晚于现在）不应出现负数或异常文案", () => {
    expect(formatRelativeTime(ago(-5 * MINUTE), NOW)).toBe("刚刚");
    expect(formatRelativeTime(ago(-3 * DAY), NOW)).toBe("刚刚");
  });

  it("非法输入返回空串而不是 NaN/Invalid Date", () => {
    expect(formatRelativeTime("", NOW)).toBe("");
    expect(formatRelativeTime("not-a-date", NOW)).toBe("");
  });
});

describe("formatDateTime", () => {
  it("格式化为中文年月日时分", () => {
    const out = formatDateTime("2026-10-14T12:00:00.000Z");
    expect(out).toMatch(/2026/);
    expect(out).toMatch(/10/);
    expect(out).toMatch(/14/);
  });

  it("非法输入返回空串", () => {
    expect(formatDateTime("")).toBe("");
    expect(formatDateTime("garbage")).toBe("");
  });
});
