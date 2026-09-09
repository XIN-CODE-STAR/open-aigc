<script setup lang="ts">
/**
 * PrismaticBurst：基于 ogl 的全屏 WebGL 棱镜爆发特效。
 *
 * 来源：用户提供的 React + TypeScript 参考实现
 * （`PrismaticBurst.tsx` + `PrismaticBurst.css`）。本组件把它以 1:1 行为
 * 移植到 Vue 3 Composition API，保留所有 shader / uniform / 动画控制逻辑。
 *
 * 用作创意工坊工作区的全屏背景层，混合模式 `lighten` 让光带叠在深色
 * 背景上不抢戏。
 */
import { onMounted, onUnmounted, ref, shallowRef, watch, type CSSProperties } from "vue";
import { Mesh, Program, Renderer, Texture, Triangle } from "ogl";

interface Offset {
  x?: number | string;
  y?: number | string;
}
type AnimationType = "rotate" | "rotate3d" | "hover";

const props = withDefaults(
  defineProps<{
    intensity?: number;
    speed?: number;
    animationType?: AnimationType;
    colors?: string[];
    distort?: number;
    paused?: boolean;
    offset?: Offset;
    hoverDampness?: number;
    rayCount?: number;
    mixBlendMode?: CSSProperties["mixBlendMode"] | "none";
  }>(),
  {
    intensity: 2,
    speed: 0.5,
    animationType: "rotate3d",
    colors: undefined,
    distort: 0,
    paused: false,
    offset: () => ({ x: 0, y: 0 }),
    hoverDampness: 0,
    rayCount: undefined,
    mixBlendMode: "lighten",
  },
);

const vertexShader = `#version 300 es
in vec2 position;
in vec2 uv;
out vec2 vUv;
void main() {
    vUv = uv;
    gl_Position = vec4(position, 0.0, 1.0);
}
`;

const fragmentShader = `#version 300 es
precision highp float;
precision highp int;

out vec4 fragColor;

uniform vec2  uResolution;
uniform float uTime;

uniform float uIntensity;
uniform float uSpeed;
uniform int   uAnimType;
uniform vec2  uMouse;
uniform int   uColorCount;
uniform float uDistort;
uniform vec2  uOffset;
uniform sampler2D uGradient;
uniform float uNoiseAmount;
uniform int   uRayCount;

float hash21(vec2 p){
    p = floor(p);
    float f = 52.9829189 * fract(dot(p, vec2(0.065, 0.005)));
    return fract(f);
}

mat2 rot30(){ return mat2(0.8, -0.5, 0.5, 0.8); }

float layeredNoise(vec2 fragPx){
    vec2 p = mod(fragPx + vec2(uTime * 30.0, -uTime * 21.0), 1024.0);
    vec2 q = rot30() * p;
    float n = 0.0;
    n += 0.40 * hash21(q);
    n += 0.25 * hash21(q * 2.0 + 17.0);
    n += 0.20 * hash21(q * 4.0 + 47.0);
    n += 0.10 * hash21(q * 8.0 + 113.0);
    n += 0.05 * hash21(q * 16.0 + 191.0);
    return n;
}

vec3 rayDir(vec2 frag, vec2 res, vec2 offset, float dist){
    float focal = res.y * max(dist, 1e-3);
    return normalize(vec3(2.0 * (frag - offset) - res, focal));
}

float edgeFade(vec2 frag, vec2 res, vec2 offset){
    vec2 toC = frag - 0.5 * res - offset;
    float r = length(toC) / (0.5 * min(res.x, res.y));
    float x = clamp(r, 0.0, 1.0);
    float q = x * x * x * (x * (x * 6.0 - 15.0) + 10.0);
    float s = q * 0.5;
    s = pow(s, 1.5);
    float tail = 1.0 - pow(1.0 - s, 2.0);
    s = mix(s, tail, 0.2);
    float dn = (layeredNoise(frag * 0.15) - 0.5) * 0.0015 * s;
    return clamp(s + dn, 0.0, 1.0);
}

mat3 rotX(float a){ float c = cos(a), s = sin(a); return mat3(1.0,0.0,0.0, 0.0,c,-s, 0.0,s,c); }
mat3 rotY(float a){ float c = cos(a), s = sin(a); return mat3(c,0.0,s, 0.0,1.0,0.0, -s,0.0,c); }
mat3 rotZ(float a){ float c = cos(a), s = sin(a); return mat3(c,-s,0.0, s,c,0.0, 0.0,0.0,1.0); }

vec3 sampleGradient(float t){
    t = clamp(t, 0.0, 1.0);
    return texture(uGradient, vec2(t, 0.5)).rgb;
}

vec2 rot2(vec2 v, float a){
    float s = sin(a), c = cos(a);
    return mat2(c, -s, s, c) * v;
}

float bendAngle(vec3 q, float t){
    float a = 0.8 * sin(q.x * 0.55 + t * 0.6)
            + 0.7 * sin(q.y * 0.50 - t * 0.5)
            + 0.6 * sin(q.z * 0.60 + t * 0.7);
    return a;
}

void main(){
    vec2 frag = gl_FragCoord.xy;
    float t = uTime * uSpeed;
    float jitterAmp = 0.1 * clamp(uNoiseAmount, 0.0, 1.0);
    vec3 dir = rayDir(frag, uResolution, uOffset, 1.0);
    float marchT = 0.0;
    vec3 col = vec3(0.0);
    float n = layeredNoise(frag);
    vec4 c = cos(t * 0.2 + vec4(0.0, 33.0, 11.0, 0.0));
    mat2 M2 = mat2(c.x, c.y, c.z, c.w);
    float amp = clamp(uDistort, 0.0, 50.0) * 0.15;

    mat3 rot3dMat = mat3(1.0);
    if(uAnimType == 1){
      vec3 ang = vec3(t * 0.31, t * 0.21, t * 0.17);
      rot3dMat = rotZ(ang.z) * rotY(ang.y) * rotX(ang.x);
    }
    mat3 hoverMat = mat3(1.0);
    if(uAnimType == 2){
      vec2 m = uMouse * 2.0 - 1.0;
      vec3 ang = vec3(m.y * 0.6, m.x * 0.6, 0.0);
      hoverMat = rotY(ang.y) * rotX(ang.x);
    }

    for (int i = 0; i < 44; ++i) {
        vec3 P = marchT * dir;
        P.z -= 2.0;
        float rad = length(P);
        vec3 Pl = P * (10.0 / max(rad, 1e-6));

        if(uAnimType == 0){
            Pl.xz *= M2;
        } else if(uAnimType == 1){
      Pl = rot3dMat * Pl;
        } else {
      Pl = hoverMat * Pl;
        }

        float stepLen = min(rad - 0.3, n * jitterAmp) + 0.1;

        float grow = smoothstep(0.35, 3.0, marchT);
        float a1 = amp * grow * bendAngle(Pl * 0.6, t);
        float a2 = 0.5 * amp * grow * bendAngle(Pl.zyx * 0.5 + 3.1, t * 0.9);
        vec3 Pb = Pl;
        Pb.xz = rot2(Pb.xz, a1);
        Pb.xy = rot2(Pb.xy, a2);

        float rayPattern = smoothstep(
            0.5, 0.7,
            sin(Pb.x + cos(Pb.y) * cos(Pb.z)) *
            sin(Pb.z + sin(Pb.y) * cos(Pb.x + t))
        );

        if (uRayCount > 0) {
            float ang = atan(Pb.y, Pb.x);
            float comb = 0.5 + 0.5 * cos(float(uRayCount) * ang);
            comb = pow(comb, 3.0);
            rayPattern *= smoothstep(0.15, 0.95, comb);
        }

        vec3 spectralDefault = 1.0 + vec3(
            cos(marchT * 3.0 + 0.0),
            cos(marchT * 3.0 + 1.0),
            cos(marchT * 3.0 + 2.0)
        );

        float saw = fract(marchT * 0.25);
        float tRay = saw * saw * (3.0 - 2.0 * saw);
        vec3 userGradient = 2.0 * sampleGradient(tRay);
        vec3 spectral = (uColorCount > 0) ? userGradient : spectralDefault;
        vec3 base = (0.05 / (0.4 + stepLen))
                  * smoothstep(5.0, 0.0, rad)
                  * spectral;

        col += base * rayPattern;
        marchT += stepLen;
    }

    col *= edgeFade(frag, uResolution, uOffset);
    col *= uIntensity;

    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}`;

const hexToRgb01 = (hex: string): [number, number, number] => {
  let h = hex.trim();
  if (h.startsWith("#")) h = h.slice(1);
  if (h.length === 3) {
    const r = h[0];
    const g = h[1];
    const b = h[2];
    h = r + r + g + g + b + b;
  }
  const intVal = parseInt(h, 16);
  if (Number.isNaN(intVal) || (h.length !== 6 && h.length !== 8)) return [1, 1, 1];
  const r = ((intVal >> 16) & 255) / 255;
  const g = ((intVal >> 8) & 255) / 255;
  const b = (intVal & 255) / 255;
  return [r, g, b];
};

const toPx = (v: number | string | undefined): number => {
  if (v == null) return 0;
  if (typeof v === "number") return v;
  const s = String(v).trim();
  const num = parseFloat(s.replace("px", ""));
  return Number.isNaN(num) ? 0 : num;
};

const containerRef = ref<HTMLDivElement | null>(null);
const programRef = shallowRef<Program | null>(null);
const rendererRef = shallowRef<Renderer | null>(null);
const gradTexRef = shallowRef<Texture | null>(null);
const meshRef = shallowRef<Mesh | null>(null);
const triRef = shallowRef<Triangle | null>(null);

const mouseTargetRef: [number, number] = [0.5, 0.5];
const mouseSmoothRef: [number, number] = [0.5, 0.5];
const pausedRef = ref(props.paused);
const hoverDampRef = ref(props.hoverDampness);
const isVisibleRef = ref(true);

let resizeObserver: ResizeObserver | null = null;
let intersectionObserver: IntersectionObserver | null = null;
let raf = 0;
let cleanupFns: (() => void)[] = [];

const setCanvasBlend = (mode: CSSProperties["mixBlendMode"] | "none" | undefined): void => {
  const canvas = rendererRef.value?.gl?.canvas as HTMLCanvasElement | undefined;
  if (!canvas) return;
  canvas.style.mixBlendMode = mode && mode !== "none" ? String(mode) : "";
};

watch(
  () => props.paused,
  (v) => {
    pausedRef.value = v;
  },
);
watch(
  () => props.hoverDampness,
  (v) => {
    hoverDampRef.value = v;
  },
);
watch(
  () => props.mixBlendMode,
  (mode) => setCanvasBlend(mode),
);

const init = (): void => {
  const container = containerRef.value;
  if (!container) return;

  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const renderer = new Renderer({ dpr, alpha: false, antialias: false });
  rendererRef.value = renderer;

  const gl = renderer.gl;
  gl.canvas.style.position = "absolute";
  gl.canvas.style.inset = "0";
  gl.canvas.style.width = "100%";
  gl.canvas.style.height = "100%";
  setCanvasBlend(props.mixBlendMode);
  container.appendChild(gl.canvas);

  const white = new Uint8Array([255, 255, 255, 255]);
  const gradientTex = new Texture(gl, {
    image: white,
    width: 1,
    height: 1,
    generateMipmaps: false,
    flipY: false,
  });
  gradientTex.minFilter = gl.LINEAR;
  gradientTex.magFilter = gl.LINEAR;
  gradientTex.wrapS = gl.CLAMP_TO_EDGE;
  gradientTex.wrapT = gl.CLAMP_TO_EDGE;
  gradTexRef.value = gradientTex;

  const program = new Program(gl, {
    vertex: vertexShader,
    fragment: fragmentShader,
    uniforms: {
      uResolution: { value: [1, 1] as [number, number] },
      uTime: { value: 0 },
      uIntensity: { value: 1 },
      uSpeed: { value: 1 },
      uAnimType: { value: 0 },
      uMouse: { value: [0.5, 0.5] as [number, number] },
      uColorCount: { value: 0 },
      uDistort: { value: 0 },
      uOffset: { value: [0, 0] as [number, number] },
      uGradient: { value: gradientTex },
      uNoiseAmount: { value: 0.8 },
      uRayCount: { value: 0 },
    },
  });
  programRef.value = program;

  const triangle = new Triangle(gl);
  const mesh = new Mesh(gl, { geometry: triangle, program });
  triRef.value = triangle;
  meshRef.value = mesh;

  const resize = (): void => {
    const w = container.clientWidth || 1;
    const h = container.clientHeight || 1;
    renderer.setSize(w, h);
    const u = program.uniforms.uResolution as { value: [number, number] };
    u.value = [gl.drawingBufferWidth, gl.drawingBufferHeight];
  };

  // 目标环境（Electron + WebView2 / 现代 Chromium）原生支持 ResizeObserver。
  // 保留为条件分支以防御 SSR / 老旧环境。
  const hasResizeObserver = typeof ResizeObserver !== "undefined";
  if (hasResizeObserver) {
    resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(container);
  } else {
    const onResize = (): void => resize();
    const w: Window = window;
    w.addEventListener("resize", onResize);
    cleanupFns.push(() => w.removeEventListener("resize", onResize));
  }
  resize();

  const onPointer = (e: PointerEvent): void => {
    const rect = container.getBoundingClientRect();
    const x = (e.clientX - rect.left) / Math.max(rect.width, 1);
    const y = (e.clientY - rect.top) / Math.max(rect.height, 1);
    mouseTargetRef[0] = Math.min(Math.max(x, 0), 1);
    mouseTargetRef[1] = Math.min(Math.max(y, 0), 1);
  };
  container.addEventListener("pointermove", onPointer, { passive: true });
  cleanupFns.push(() => container.removeEventListener("pointermove", onPointer));

  if ("IntersectionObserver" in window) {
    intersectionObserver = new IntersectionObserver(
      (entries) => {
        const first = entries[0];
        if (first) isVisibleRef.value = first.isIntersecting;
      },
      { root: null, threshold: 0.01 },
    );
    intersectionObserver.observe(container);
  }

  let last = performance.now();
  let accumTime = 0;

  const update = (now: number): void => {
    const dt = Math.max(0, now - last) * 0.001;
    last = now;
    const visible = isVisibleRef.value && !document.hidden;
    if (!pausedRef.value) accumTime += dt;
    if (!visible) {
      raf = requestAnimationFrame(update);
      return;
    }
    const tau = 0.02 + Math.max(0, Math.min(1, hoverDampRef.value)) * 0.5;
    const alpha = 1 - Math.exp(-dt / tau);
    const tgt = mouseTargetRef;
    const sm = mouseSmoothRef;
    sm[0] += (tgt[0] - sm[0]) * alpha;
    sm[1] += (tgt[1] - sm[1]) * alpha;
    const u = program.uniforms.uMouse as { value: [number, number] };
    u.value = sm;
    const ut = program.uniforms.uTime as { value: number };
    ut.value = accumTime;
    const m = meshRef.value;
    if (m) renderer.render({ scene: m });
    raf = requestAnimationFrame(update);
  };
  raf = requestAnimationFrame(update);
};

const dispose = (): void => {
  if (raf) cancelAnimationFrame(raf);
  raf = 0;
  resizeObserver?.disconnect();
  resizeObserver = null;
  intersectionObserver?.disconnect();
  intersectionObserver = null;
  cleanupFns.forEach((fn) => fn());
  cleanupFns = [];
  const renderer = rendererRef.value;
  const program = programRef.value;
  const gradTex = gradTexRef.value;
  if (renderer) {
    try {
      const canvas = renderer.gl?.canvas as HTMLCanvasElement | undefined;
      if (canvas?.parentElement) canvas.parentElement.removeChild(canvas);
    } catch {
      /* 节点已被卸载，忽略 */
    }
  }
  if (renderer && gradTex) {
    try {
      const tex = (gradTex as unknown as { texture?: WebGLTexture }).texture;
      if (tex) renderer.gl?.deleteTexture(tex);
    } catch {
      /* 忽略 */
    }
  }
  programRef.value = null;
  rendererRef.value = null;
  gradTexRef.value = null;
  meshRef.value = null;
  triRef.value = null;
  // 抑制未使用变量告警
  void program;
  void triRef.value;
};

onMounted(() => {
  init();
});

onUnmounted(() => {
  dispose();
});

// 响应 props 变化（颜色、动画类型、速度、强度等）
watch(
  () =>
    [
      props.intensity,
      props.speed,
      props.animationType,
      props.colors,
      props.distort,
      props.offset?.x,
      props.offset?.y,
      props.rayCount,
    ] as const,
  () => {
    const program = programRef.value;
    const renderer = rendererRef.value;
    const gradTex = gradTexRef.value;
    if (!program || !renderer || !gradTex) return;

    const u = program.uniforms as Record<string, { value: unknown }>;
    u.uIntensity.value = props.intensity ?? 1;
    u.uSpeed.value = props.speed ?? 1;

    const animTypeMap: Record<AnimationType, number> = {
      rotate: 0,
      rotate3d: 1,
      hover: 2,
    };
    u.uAnimType.value = animTypeMap[props.animationType ?? "rotate"];

    u.uDistort.value = typeof props.distort === "number" ? props.distort : 0;

    const ox = toPx(props.offset?.x);
    const oy = toPx(props.offset?.y);
    (u.uOffset as { value: [number, number] }).value = [ox, oy];
    (u.uRayCount as { value: number }).value = Math.max(0, Math.floor(props.rayCount ?? 0));

    let count = 0;
    if (Array.isArray(props.colors) && props.colors.length > 0) {
      const gl = renderer.gl;
      const capped = props.colors.slice(0, 64);
      count = capped.length;
      const data = new Uint8Array(count * 4);
      for (let i = 0; i < count; i++) {
        const [r, g, b] = hexToRgb01(capped[i]);
        data[i * 4 + 0] = Math.round(r * 255);
        data[i * 4 + 1] = Math.round(g * 255);
        data[i * 4 + 2] = Math.round(b * 255);
        data[i * 4 + 3] = 255;
      }
      const tex = gradTex as unknown as {
        image: Uint8Array;
        width: number;
        height: number;
        minFilter: number;
        magFilter: number;
        wrapS: number;
        wrapT: number;
        flipY: boolean;
        generateMipmaps: boolean;
        format: number;
        type: number;
        needsUpdate: boolean;
      };
      tex.image = data;
      tex.width = count;
      tex.height = 1;
      tex.minFilter = gl.LINEAR;
      tex.magFilter = gl.LINEAR;
      tex.wrapS = gl.CLAMP_TO_EDGE;
      tex.wrapT = gl.CLAMP_TO_EDGE;
      tex.flipY = false;
      tex.generateMipmaps = false;
      tex.format = gl.RGBA;
      tex.type = gl.UNSIGNED_BYTE;
      tex.needsUpdate = true;
    } else {
      count = 0;
    }
    (u.uColorCount as { value: number }).value = count;
  },
  { immediate: true },
);
</script>

<template>
  <div ref="containerRef" class="prismatic-burst-container" />
</template>

<style scoped>
.prismatic-burst-container {
  position: relative;
  width: 100%;
  height: 100%;
  overflow: hidden;
}
</style>
