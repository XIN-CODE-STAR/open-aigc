<template>
  <div class="demo-container">
    <h1>PrismaticBurst 组件演示</h1>

    <!-- 基础示例 -->
    <section class="demo-section">
      <h2>基础 3D 旋转效果</h2>
      <div class="demo-box">
        <PrismaticBurst
          :intensity="2"
          :speed="0.5"
          animation-type="rotate3d"
          :colors="['#ff007a', '#4d3dff', '#ffffff']"
        />
      </div>
    </section>

    <!-- 自定义颜色 -->
    <section class="demo-section">
      <h2>自定义渐变色</h2>
      <div class="demo-box">
        <PrismaticBurst
          :intensity="3"
          :speed="0.8"
          animation-type="rotate3d"
          :colors="['#A855F7', '#7C3AED', '#6366F1', '#3B82F6']"
        />
      </div>
    </section>

    <!-- 2D 旋转 -->
    <section class="demo-section">
      <h2>2D 旋转模式</h2>
      <div class="demo-box">
        <PrismaticBurst
          :intensity="1.5"
          :speed="0.3"
          animation-type="rotate"
          :colors="['#10B981', '#059669', '#047857']"
          mix-blend-mode="screen"
        />
      </div>
    </section>

    <!-- 交互式悬停 -->
    <section class="demo-section">
      <h2>鼠标悬停交互</h2>
      <p class="demo-hint">移动鼠标查看效果</p>
      <div class="demo-box">
        <PrismaticBurst
          animation-type="hover"
          :hover-dampness="0.25"
          :intensity="2"
          :colors="['#F59E0B', '#EF4444', '#EC4899']"
        />
      </div>
    </section>

    <!-- 低强度背景 -->
    <section class="demo-section">
      <h2>低强度背景效果</h2>
      <div class="demo-box">
        <PrismaticBurst
          :intensity="0.8"
          :speed="0.2"
          animation-type="rotate"
          :colors="['#6366F1', '#8B5CF6', '#A78BFA']"
          mix-blend-mode="soft-light"
        />
        <div class="demo-content">
          <p>这是覆盖在 PrismaticBurst 上的内容</p>
        </div>
      </div>
    </section>

    <!-- 自定义配置 -->
    <section class="demo-section">
      <h2>自定义配置</h2>
      <div class="demo-controls">
        <label>
          强度：
          <input v-model.number="config.intensity" type="range" min="0" max="5" step="0.1" />
          {{ config.intensity }}
        </label>
        <label>
          速度：
          <input v-model.number="config.speed" type="range" min="0" max="2" step="0.1" />
          {{ config.speed }}
        </label>
        <label>
          扭曲：
          <input v-model.number="config.distort" type="range" min="0" max="20" step="0.5" />
          {{ config.distort }}
        </label>
        <label>
          光线数：
          <input v-model.number="config.rayCount" type="range" min="0" max="16" step="1" />
          {{ config.rayCount }}
        </label>
      </div>
      <div class="demo-box">
        <PrismaticBurst
          :intensity="config.intensity"
          :speed="config.speed"
          :distort="config.distort"
          :ray-count="config.rayCount"
          animation-type="rotate3d"
          :colors="config.colors"
        />
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { reactive } from "vue";
import PrismaticBurst from "./PrismaticBurst.vue";

const config = reactive({
  intensity: 2,
  speed: 0.5,
  distort: 0,
  rayCount: 0,
  colors: ["#ff007a", "#4d3dff", "#ffffff", "#A855F7"],
});
</script>

<style scoped>
.demo-container {
  max-width: 1200px;
  margin: 0 auto;
  padding: 40px 20px;
  font-family:
    system-ui,
    -apple-system,
    sans-serif;
}

h1 {
  text-align: center;
  color: #1a1a1a;
  margin-bottom: 40px;
}

h2 {
  color: #333;
  margin-bottom: 16px;
  font-size: 1.5rem;
}

.demo-section {
  margin-bottom: 48px;
}

.demo-box {
  position: relative;
  width: 100%;
  height: 400px;
  border-radius: 12px;
  overflow: hidden;
  background: #0a0a0a;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.15);
}

.demo-hint {
  color: #666;
  font-size: 14px;
  margin-bottom: 12px;
  font-style: italic;
}

.demo-content {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: white;
  font-size: 24px;
  font-weight: 600;
  text-shadow: 0 2px 10px rgba(0, 0, 0, 0.5);
}

.demo-controls {
  display: flex;
  flex-wrap: wrap;
  gap: 20px;
  margin-bottom: 20px;
  padding: 20px;
  background: #f5f5f5;
  border-radius: 8px;
}

.demo-controls label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  color: #555;
}

.demo-controls input[type="range"] {
  width: 120px;
  height: 6px;
  border-radius: 3px;
  background: #ddd;
  outline: none;
}

.demo-controls input[type="range"]::-webkit-slider-thumb {
  appearance: none;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #6366f1;
  cursor: pointer;
}

@media (max-width: 768px) {
  .demo-box {
    height: 300px;
  }

  .demo-controls {
    flex-direction: column;
    gap: 12px;
  }
}
</style>
