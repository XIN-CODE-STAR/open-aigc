<script setup lang="ts">
/**
 * AuroraCanvas：全局极光辉光背景。
 *
 * 纯 CSS 动画实现，无 WebGL 开销。
 * 三个缓慢漂移的渐变光球 + 噪点纹理叠加，
 * 营造"光存在于空间中"的氛围感。
 *
 * 用法：作为 AppShell 或页面的最底层，设置 pointer-events: none。
 */
defineProps<{
  /** 是否暂停动画（不可见时节省性能）。 */
  paused?: boolean;
}>();
</script>

<template>
  <div class="aurora-canvas" :class="{ 'is-paused': paused }">
    <div class="aurora-orb aurora-orb--violet" />
    <div class="aurora-orb aurora-orb--cyan" />
    <div class="aurora-orb aurora-orb--pink" />
    <div class="aurora-noise" />
  </div>
</template>

<style scoped>
.aurora-canvas {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
  z-index: 0;
  /* 亮色主题下隐藏极光 */
  opacity: 0;
}

:root[data-theme="dark"] .aurora-canvas {
  opacity: 1;
}

.aurora-canvas.is-paused .aurora-orb,
.aurora-canvas.is-paused .aurora-noise {
  animation-play-state: paused;
}

/* —— 光球基础 —— */
.aurora-orb {
  position: absolute;
  border-radius: 50%;
  filter: blur(100px);
  will-change: transform;
  mix-blend-mode: screen;
}

/* —— 紫罗兰光球 · 左上 —— */
.aurora-orb--violet {
  width: 50vw;
  height: 50vw;
  max-width: 600px;
  max-height: 600px;
  top: -15%;
  left: -10%;
  background: radial-gradient(
    circle,
    rgba(139, 92, 246, 0.18) 0%,
    rgba(139, 92, 246, 0.06) 40%,
    transparent 70%
  );
  animation: aurora-drift-1 25s ease-in-out infinite alternate;
}

/* —— 青色光球 · 右下 —— */
.aurora-orb--cyan {
  width: 45vw;
  height: 45vw;
  max-width: 550px;
  max-height: 550px;
  bottom: -20%;
  right: -15%;
  background: radial-gradient(
    circle,
    rgba(6, 182, 212, 0.14) 0%,
    rgba(6, 182, 212, 0.05) 40%,
    transparent 70%
  );
  animation: aurora-drift-2 30s ease-in-out infinite alternate;
}

/* —— 粉色光球 · 中央偏右 —— */
.aurora-orb--pink {
  width: 35vw;
  height: 35vw;
  max-width: 450px;
  max-height: 450px;
  top: 30%;
  right: 20%;
  background: radial-gradient(
    circle,
    rgba(236, 72, 153, 0.1) 0%,
    rgba(236, 72, 153, 0.04) 40%,
    transparent 70%
  );
  animation: aurora-drift-3 20s ease-in-out infinite alternate;
}

/* —— 噪点纹理叠加 —— */
.aurora-noise {
  position: absolute;
  inset: 0;
  opacity: 0.03;
  background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 256 256' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='4' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
  background-size: 200px 200px;
  mix-blend-mode: overlay;
}

/* —— 漂移动画 —— */
@keyframes aurora-drift-1 {
  0% {
    transform: translate(0, 0) scale(1);
  }
  33% {
    transform: translate(8vw, 5vh) scale(1.05);
  }
  66% {
    transform: translate(-3vw, 10vh) scale(0.95);
  }
  100% {
    transform: translate(5vw, -3vh) scale(1.08);
  }
}

@keyframes aurora-drift-2 {
  0% {
    transform: translate(0, 0) scale(1);
  }
  33% {
    transform: translate(-6vw, -8vh) scale(1.08);
  }
  66% {
    transform: translate(4vw, -4vh) scale(0.96);
  }
  100% {
    transform: translate(-2vw, 6vh) scale(1.04);
  }
}

@keyframes aurora-drift-3 {
  0% {
    transform: translate(0, 0) scale(1) rotate(0deg);
  }
  50% {
    transform: translate(-10vw, 8vh) scale(1.1) rotate(15deg);
  }
  100% {
    transform: translate(5vw, -5vh) scale(0.9) rotate(-10deg);
  }
}

@media (prefers-reduced-motion: reduce) {
  .aurora-orb {
    animation: none;
  }
}
</style>
