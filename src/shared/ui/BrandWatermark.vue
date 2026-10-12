<script setup lang="ts">
/**
 * BrandWatermark：品牌水印层 —— 客户端 Logo「A」居中 + 同心回声层层递进。
 * 纯装饰：absolute inset-0、pointer-events:none、aria-hidden。
 * 颜色走 --color-accent-text（浅色=板岩调、深色=白），随主题自动适配。
 * 宿主负责定位上下文与底色；本组件只铺水印，铺满宿主。
 */
</script>

<template>
  <div class="brand-watermark" aria-hidden="true">
    <svg class="brand-watermark__mark brand-watermark__mark--l1" viewBox="0 0 512 512">
      <path d="M256 92 120 404h72l24-60h80l24 60h72L256 92Zm0 148-20 50h40l-20-50Z" />
    </svg>
    <svg class="brand-watermark__mark brand-watermark__mark--l2" viewBox="0 0 512 512">
      <path d="M256 92 120 404h72l24-60h80l24 60h72L256 92Zm0 148-20 50h40l-20-50Z" />
    </svg>
    <svg class="brand-watermark__mark brand-watermark__mark--l3" viewBox="0 0 512 512">
      <path d="M256 92 120 404h72l24-60h80l24 60h72L256 92Zm0 148-20 50h40l-20-50Z" />
    </svg>
    <svg class="brand-watermark__mark brand-watermark__mark--l4" viewBox="0 0 512 512">
      <path d="M256 92 120 404h72l24-60h80l24 60h72L256 92Zm0 148-20 50h40l-20-50Z" />
    </svg>
  </div>
</template>

<style scoped>
.brand-watermark {
  position: absolute;
  inset: 0;
  overflow: hidden;
  color: var(--color-accent-text);
  pointer-events: none;
}

.brand-watermark__mark {
  position: absolute;
  top: 50%;
  left: 50%;
  translate: -50% -50%;
  fill: currentColor;
}

/* 主体：线框描边的大 A */
.brand-watermark__mark--l1 {
  width: 42vmin;
  opacity: 0.08;
}

.brand-watermark__mark--l1 path {
  fill: none;
  stroke: currentColor;
  stroke-width: 13;
}

/* 回声层：同一圆心，向外逐级放大、变淡 —— 层层递进 */
.brand-watermark__mark--l2 {
  width: 58vmin;
  opacity: 0.038;
}

.brand-watermark__mark--l3 {
  width: 78vmin;
  opacity: 0.022;
}

.brand-watermark__mark--l4 {
  width: 102vmin;
  opacity: 0.012;
}
</style>
