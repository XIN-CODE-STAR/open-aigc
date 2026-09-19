/**
 * 画布节点尺寸工具。
 *
 * 自定义尺寸持久化在 payload_json 的 size 字段：memory_nodes 表虽有
 * width/height 列，但 memory_node_v1_update 不透传（Phase 2 数据层重构
 * 时再迁移），因此在不改后端的前提下，尺寸随 payload 一起落库。
 */

export interface CanvasNodeSize {
  width: number;
  /** 未设置时节点高度由内容撑开。 */
  height?: number;
}

export const NODE_SIZE_LIMITS = {
  minWidth: 120,
  minHeight: 80,
  maxWidth: 960,
  maxHeight: 960,
  defaultWidth: 220,
} as const;

interface NodeSizeSource {
  payloadJson: string;
  width: number | null;
  height: number | null;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, Math.round(value)));
}

export function clampNodeSize(width: number, height?: number): CanvasNodeSize {
  const size: CanvasNodeSize = {
    width: clamp(width, NODE_SIZE_LIMITS.minWidth, NODE_SIZE_LIMITS.maxWidth),
  };
  if (height !== undefined) {
    size.height = clamp(height, NODE_SIZE_LIMITS.minHeight, NODE_SIZE_LIMITS.maxHeight);
  }
  return size;
}

/** 节点有效尺寸：payload.size 优先，其次 DB 列（历史数据），最后默认宽度。 */
export function getEffectiveSize(node: NodeSizeSource): CanvasNodeSize {
  try {
    const payload = JSON.parse(node.payloadJson) as { size?: Partial<CanvasNodeSize> };
    if (payload.size && typeof payload.size.width === "number") {
      const height = typeof payload.size.height === "number" ? payload.size.height : undefined;
      return clampNodeSize(payload.size.width, height);
    }
  } catch {
    // payload 非法时回退到列值
  }
  if (node.width && node.width > 0) {
    return clampNodeSize(node.width, node.height && node.height > 0 ? node.height : undefined);
  }
  return { width: NODE_SIZE_LIMITS.defaultWidth };
}

/** 把尺寸写入 payload JSON（保留其他字段），返回新 JSON 字符串。 */
export function withNodeSize(payloadJson: string, size: CanvasNodeSize): string {
  let payload: Record<string, unknown> = {};
  try {
    payload = JSON.parse(payloadJson) as Record<string, unknown>;
  } catch {
    payload = {};
  }
  payload.size = size;
  return JSON.stringify(payload);
}
