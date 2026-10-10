import { describe, expect, it } from "vitest";

import { parseAccountCredits } from "./resourceAccounts";

/** 健康检查 worker 写入 `extra_json` 的形态。 */
function record(extra: unknown): { extraJson: string } {
  return { extraJson: typeof extra === "string" ? extra : JSON.stringify(extra) };
}

describe("parseAccountCredits", () => {
  it("读出健康检查写入的积分快照", () => {
    const credits = parseAccountCredits(
      record({
        credits: { total: 66, gift: 66, purchase: 0, vip: 0 },
        creditsCheckedAt: "2026-10-10T15:11:55Z",
      }),
    );
    expect(credits).toEqual({
      total: 66,
      gift: 66,
      purchase: 0,
      vip: 0,
      checkedAt: "2026-10-10T15:11:55Z",
    });
  });

  it("缺明细时按 0 处理，只要求 total", () => {
    const credits = parseAccountCredits(record({ credits: { total: 12 } }));
    expect(credits).toEqual({ total: 12, gift: 0, purchase: 0, vip: 0, checkedAt: null });
  });

  it("没有积分信息时返回 null（老记录 / 代理不可达 / 非即梦账号）", () => {
    for (const extra of ["{}", "", "not json", { other: 1 }, { credits: {} }, { credits: null }]) {
      expect(parseAccountCredits(record(extra)), JSON.stringify(extra)).toBeNull();
    }
  });
});
