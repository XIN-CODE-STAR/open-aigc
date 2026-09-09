# PrismaticBurst 组件

WebGL 棱镜爆发效果组件，为创意工坊提供视觉吸引力的背景动画。

## 功能特性

- **实时 WebGL 渲染**：使用 OGL 库进行 GPU 加速渲染
- **多种动画模式**：支持 rotate、rotate3d 和 hover 三种动画类型
- **自定义渐变色**：支持自定义颜色数组创建渐变效果
- **性能优化**：
  - IntersectionObserver 检测可见性，避免不可见时渲染
  - 设备像素比限制（最大 2x）
  - 鼠标悬停阻尼效果
- **完全响应式**：自动适应容器尺寸变化
- **混合模式**：支持 CSS mix-blend-mode

## 使用方法

### 基本用法

```vue
<template>
  <div style="width: 100%; height: 600px; position: relative">
    <PrismaticBurst
      :intensity="2"
      :speed="0.5"
      animation-type="rotate3d"
      :distort="0"
      :paused="false"
      :offset="{ x: 0, y: 0 }"
      :hover-dampness="0.25"
      :ray-count="0"
      mix-blend-mode="lighten"
      :colors="['#ff007a', '#4d3dff', '#ffffff']"
    />
  </div>
</template>

<script setup lang="ts">
import PrismaticBurst from './PrismaticBurst.vue';
</script>
```

## Props 属性

| 属性 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `intensity` | `number` | `2` | 光线强度（0-10） |
| `speed` | `number` | `0.5` | 动画速度（0-2） |
| `animationType` | `'rotate' \| 'rotate3d' \| 'hover'` | `'rotate3d'` | 动画类型 |
| `colors` | `string[]` | `[]` | 自定义渐变色数组（十六进制颜色） |
| `distort` | `number` | `0` | 扭曲程度（0-50） |
| `paused` | `boolean` | `false` | 是否暂停动画 |
| `offset` | `{ x?: number \| string; y?: number \| string }` | `{ x: 0, y: 0 }` | 光源偏移量 |
| `hoverDampness` | `number` | `0` | 鼠标悬停阻尼（0-1） |
| `rayCount` | `number` | `0` | 光线数量（0 为自动） |
| `mixBlendMode` | `string` | `'lighten'` | CSS 混合模式 |

## 动画类型

### rotate
2D 旋转动画，光线围绕中心点旋转。

### rotate3d
3D 旋转动画，光线在三个维度上旋转，营造立体感。

### hover
鼠标跟随动画，光线跟随鼠标移动，需要设置 `hoverDampness` 控制响应灵敏度。

## 示例

### 紫色主题
```vue
<PrismaticBurst
  :colors="['#A855F7', '#7C3AED', '#6366F1']"
  :intensity="2"
  animation-type="rotate3d"
/>
```

### 彩虹效果
```vue
<PrismaticBurst
  :colors="['#ff0000', '#ff7700', '#ffff00', '#00ff00', '#0000ff', '#8b00ff']"
  :intensity="3"
  :speed="0.8"
/>
```

### 低强度背景
```vue
<PrismaticBurst
  :intensity="0.5"
  :speed="0.3"
  animation-type="rotate"
  mix-blend-mode="soft-light"
/>
```

### 交互式悬停
```vue
<PrismaticBurst
  animation-type="hover"
  :hover-dampness="0.25"
  :intensity="2"
  :colors="['#ff007a', '#4d3dff']"
/>
```

## 性能建议

1. **设置合适的容器尺寸**：避免过大的渲染区域影响性能
2. **使用 IntersectionObserver**：组件自动实现，不可见时停止渲染
3. **限制设备像素比**：自动限制为最大 2x
4. **合理使用 paused**：在不需要动画时暂停渲染
5. **避免过多颜色**：颜色数量建议不超过 64 个

## 依赖

- `ogl`: WebGL 渲染库（已在项目中安装）

## 集成到创意工坊

该组件已集成到 `GenerationsPage.vue` 中作为背景层：

```vue
<div class="grok-shell">
  <!-- WebGL 棱镜爆发背景层 -->
  <PrismaticBurst
    :intensity="2"
    :speed="0.5"
    animation-type="rotate3d"
    :distort="0"
    :paused="false"
    :offset="{ x: 0, y: 0 }"
    :hover-dampness="0.25"
    :ray-count="0"
    mix-blend-mode="lighten"
    :colors="['#ff007a', '#4d3dff', '#ffffff']"
    class="grok-shell__burst"
  />
  <!-- 主内容区域 -->
  <div class="grok-main">
    <!-- ... -->
  </div>
</div>
```

样式设置：
```css
.grok-shell__burst {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  z-index: 0;
  pointer-events: none;
}
```

## 技术实现

- 使用 WebGL 2.0 着色器（GLSL ES 3.0）
- 光线步进（ray marching）算法
- 多层噪声叠加
- 边缘衰减效果
- 实时鼠标交互

## 浏览器支持

- Chrome 56+
- Firefox 51+
- Safari 15+
- Edge 79+

需要 WebGL 2.0 支持。

## 注意事项

1. 该组件会创建一个 canvas 元素并覆盖整个容器
2. 使用 `pointer-events: none` 确保不影响下层元素的交互
3. 在移动设备上可能会影响性能，建议适当降低 `intensity` 和 `speed`
4. 组件会在卸载时自动清理所有 WebGL 资源

## 更新日志

### 1.0.0
- 从 React 版本转换为 Vue 3 Composition API
- 集成到创意工坊页面
- 优化 TypeScript 类型定义
- 改进清理逻辑
