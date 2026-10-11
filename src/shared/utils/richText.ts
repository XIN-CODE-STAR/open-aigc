/**
 * 富文本渲染工具（跨模块共享）。
 *
 * 只做一件受控的事：把自有 LLM 输出的 markdown 媒体语法（`![alt](url)`）
 * 转换为 `<img>` / `<video>` 标签。内容来自自有 LLM，属可信来源；这里仍不解析
 * 任意 HTML，只做定点替换，避免引入额外的 XSS 面。
 */

/** 视频扩展名：markdown 图片语法中带这些扩展名时渲染为 `<video>`。 */
const VIDEO_EXT = /\.(mp4|webm|mov|m4v)(\?|#|$)/i;

/** 判断 URL 是否指向视频资源。 */
export function isVideoUrl(url: string): boolean {
  return VIDEO_EXT.test(url);
}

/**
 * 渲染助手消息内容：把 markdown 图片/视频语法替换为对应媒体标签。
 * 其余文本原样保留（由调用方决定是否作为 HTML 呈现）。
 */
export function renderRichContent(content: string): string {
  return content.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (_match, alt: string, url: string) => {
    if (VIDEO_EXT.test(url)) {
      return `<video src="${url}" controls preload="metadata" style="max-width:100%;max-height:400px;border-radius:8px;margin:8px 0;display:block;"></video>`;
    }
    return `<img src="${url}" alt="${alt}" style="max-width:100%;max-height:400px;border-radius:8px;margin:8px 0;display:block;object-fit:contain;" />`;
  });
}

/**
 * 解析用户消息内容。
 * 后端将含附件的消息存为 JSON：`{"images": [...], "text": "..."}`；
 * 纯文本消息原样返回。返回值拆开 text 与 images，避免 base64 出现在文字区。
 */
export function parseUserMessageContent(raw: string | null): { text: string; images: string[] } {
  if (!raw) return { text: "", images: [] };
  try {
    const data = JSON.parse(raw);
    if (typeof data === "object" && data !== null && typeof data.text === "string") {
      const images = Array.isArray(data.images)
        ? data.images.filter((item: unknown) => typeof item === "string" && item.length > 0)
        : [];
      return { text: data.text, images };
    }
  } catch {
    // 非 JSON，按纯文本处理
  }
  return { text: raw, images: [] };
}
