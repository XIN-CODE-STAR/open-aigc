/**
 * 时间格式化工具：相对时间（列表用）+ 绝对时间（tooltip 用）。
 */

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/**
 * 相对时间：刚刚 / N 分钟前 / N 小时前 / N 天前 / 具体日期。
 * 用于任务列表右侧的时间列（对齐参考设计：16小时前、3天前）。
 */
export function formatRelativeTime(iso: string, now: Date = new Date()): string {
  const time = new Date(iso);
  if (Number.isNaN(time.getTime())) return "";
  const diff = now.getTime() - time.getTime();

  if (diff < MINUTE) return "刚刚";
  if (diff < HOUR) return `${Math.floor(diff / MINUTE)}分钟前`;
  if (diff < DAY) return `${Math.floor(diff / HOUR)}小时前`;
  if (diff < 30 * DAY) return `${Math.floor(diff / DAY)}天前`;

  const sameYear = time.getFullYear() === now.getFullYear();
  return sameYear
    ? `${time.getMonth() + 1}月${time.getDate()}日`
    : `${time.getFullYear()}年${time.getMonth() + 1}月${time.getDate()}日`;
}

/** 绝对时间：用于 title 提示，补充相对时间缺失的精确信息。 */
export function formatDateTime(iso: string): string {
  const time = new Date(iso);
  if (Number.isNaN(time.getTime())) return "";
  return time.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}
