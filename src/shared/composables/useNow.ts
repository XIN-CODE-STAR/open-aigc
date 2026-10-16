import { onScopeDispose, ref } from "vue";

/**
 * 会自己走的「现在」。
 *
 * 相对时间（"3小时前"）有个容易漏的问题：如果在渲染时直接把 `new Date()`
 * 传给格式化函数，「现在」就被焊死在这一帧上。桌面应用一开就是一整天，
 * 列表又不总因别的原因重渲染，于是标签停在过期分桶里 —— 明明已经两天了
 * 还写着「1天前」。
 *
 * 这里用一个定时更新的 ref 当「现在」，页面只要在模板里引用它，跨过分桶
 * 边界时就会自动重算。
 *
 * @param intervalMs 刷新间隔，默认 30 秒（相对时间的最小分桶是 1 分钟，
 *                   刷得再勤也不会更细，白费性能）。
 */
export function useNow(intervalMs = 30_000) {
  const now = ref(new Date());

  const timer = window.setInterval(() => {
    now.value = new Date();
  }, intervalMs);

  // 组件卸载 / effect scope 销毁时停表，避免长开会话里堆定时器
  onScopeDispose(() => {
    window.clearInterval(timer);
  });

  return now;
}
