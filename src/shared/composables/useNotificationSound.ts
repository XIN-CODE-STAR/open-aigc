/**
 * 通知提示音：使用 Web Audio API 合成短促提示音。
 *
 * 无需音频文件，不受 CSP media-src 限制。
 * 两种音色：
 * - success：上行双音（C5 → E5），表示生成完成 / Agent 回复就绪
 * - error：下行双音（A4 → F4），表示生成失败 / Agent 异常
 */

let audioContext: AudioContext | null = null;

function getContext(): AudioContext | null {
  try {
    if (!audioContext) {
      audioContext = new AudioContext();
    }
    if (audioContext.state === "suspended") {
      void audioContext.resume();
    }
    return audioContext;
  } catch {
    return null;
  }
}

/**
 * 播放一个短促的正弦音。
 */
function playTone(
  ctx: AudioContext,
  frequency: number,
  startTime: number,
  duration: number,
  volume: number,
): void {
  const oscillator = ctx.createOscillator();
  const gain = ctx.createGain();

  oscillator.type = "sine";
  oscillator.frequency.setValueAtTime(frequency, startTime);

  // 快速起音 + 自然衰减，避免爆音
  gain.gain.setValueAtTime(0, startTime);
  gain.gain.linearRampToValueAtTime(volume, startTime + 0.02);
  gain.gain.exponentialRampToValueAtTime(0.001, startTime + duration);

  oscillator.connect(gain);
  gain.connect(ctx.destination);

  oscillator.start(startTime);
  oscillator.stop(startTime + duration);
}

/**
 * 播放成功提示音（上行双音，柔和）。
 */
export function playSuccessSound(): void {
  const ctx = getContext();
  if (!ctx) return;

  const now = ctx.currentTime;
  playTone(ctx, 523.25, now, 0.15, 0.12); // C5
  playTone(ctx, 659.25, now + 0.12, 0.2, 0.1); // E5
}

/**
 * 播放失败提示音（下行双音，低沉）。
 */
export function playErrorSound(): void {
  const ctx = getContext();
  if (!ctx) return;

  const now = ctx.currentTime;
  playTone(ctx, 440.0, now, 0.15, 0.1); // A4
  playTone(ctx, 349.23, now + 0.12, 0.25, 0.08); // F4
}
