/**
 * 画布图片的尺寸与体积治理（Phase 5）。
 *
 * - 渲染端：大图用 Canvas API 生成缩略图替代原图渲染（thumbnailFor）；
 * - 持久化端：payload 即将超过 500KB 的图片自动压缩到 max 2048px（compressIfNeeded）。
 *
 * Canvas 不可用（测试环境 / 解码失败 / 压缩无效）时一律回退原图，
 * 调用方无需处理失败分支，业务语义不变。
 */

/** payload_json 中图片体积软上限（字符数 ≈ base64 字节数）。 */
export const PAYLOAD_SIZE_LIMIT = 500 * 1024;
/** 渲染缩略图的原图体积阈值。 */
export const THUMBNAIL_SOURCE_THRESHOLD = 300 * 1024;

const THUMBNAIL_MAX_EDGE = 640;
const COMPRESS_MAX_EDGE = 2048;
const COMPRESS_FALLBACK_EDGE = 1024;
const COMPRESS_QUALITY = 0.85;

const thumbnailCache = new Map<string, string>();

function isDataUrlImage(source: string): boolean {
  return source.startsWith("data:image/");
}

function canvasSupported(): boolean {
  try {
    const canvas = document.createElement("canvas");
    return canvas.getContext("2d") !== null;
  } catch {
    return false;
  }
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("image decode failed"));
    image.src = src;
  });
}

/** 把图片等比缩到 maxEdge 内并导出；返回 null 表示 Canvas 不可用。 */
function drawToDataUrl(image: HTMLImageElement, maxEdge: number, quality: number): string | null {
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d");
  if (!context) return null;
  const width = image.naturalWidth || image.width || 1;
  const height = image.naturalHeight || image.height || 1;
  const scale = Math.min(1, maxEdge / Math.max(width, height));
  canvas.width = Math.max(1, Math.round(width * scale));
  canvas.height = Math.max(1, Math.round(height * scale));
  context.drawImage(image, 0, 0, canvas.width, canvas.height);
  const webp = canvas.toDataURL("image/webp", quality);
  if (webp.startsWith("data:image/webp")) return webp;
  const jpeg = canvas.toDataURL("image/jpeg", quality);
  return jpeg.startsWith("data:image/jpeg") ? jpeg : null;
}

/** 压缩单档；压缩无效（结果更大）时抛错让调用方回退。 */
async function compressDataUrl(dataUrl: string, maxEdge: number): Promise<string> {
  const image = await loadImage(dataUrl);
  const output = drawToDataUrl(image, maxEdge, COMPRESS_QUALITY);
  if (!output || output.length >= dataUrl.length) {
    throw new Error("compression ineffective");
  }
  return output;
}

/**
 * 持久化前的体积守卫：dataUrl 超过 limit 时按 2048 → 1024 两档压缩，
 * 失败 / 非图片 / 体积不超标时原样返回。
 */
export async function compressIfNeeded(
  dataUrl: string,
  limit: number = PAYLOAD_SIZE_LIMIT,
): Promise<string> {
  if (!isDataUrlImage(dataUrl) || dataUrl.length <= limit) return dataUrl;
  if (!canvasSupported()) return dataUrl;
  try {
    const first = await compressDataUrl(dataUrl, COMPRESS_MAX_EDGE);
    if (first.length <= limit) return first;
    const second = await compressDataUrl(first, COMPRESS_FALLBACK_EDGE);
    return second.length < first.length ? second : first;
  } catch {
    return dataUrl;
  }
}

/**
 * 渲染用缩略图：大图生成 640px 缩略图替代原图渲染并按 cacheKey 缓存；
 * 小图直接返回原图，异常时回退原图。
 */
export async function thumbnailFor(source: string, cacheKey: string): Promise<string> {
  if (!isDataUrlImage(source) || source.length <= THUMBNAIL_SOURCE_THRESHOLD) return source;
  const cached = thumbnailCache.get(cacheKey);
  if (cached !== undefined) return cached;
  let result = source;
  try {
    if (canvasSupported()) {
      const image = await loadImage(source);
      result = drawToDataUrl(image, THUMBNAIL_MAX_EDGE, 0.82) ?? source;
    }
  } catch {
    result = source;
  }
  thumbnailCache.set(cacheKey, result);
  return result;
}

export function clearThumbnailCache(): void {
  thumbnailCache.clear();
}
