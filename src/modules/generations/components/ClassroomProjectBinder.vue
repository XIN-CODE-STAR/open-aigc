<script setup lang="ts">
/**
 * ClassroomProjectBinder：班级-项目关联选择器。
 *
 * 让教师将 AI 漫剧项目绑定到特定班级。
 */
import { computed, ref, watch } from "vue";
import { BookOpen, Check, ChevronDown, Link2 } from "@lucide/vue";

interface Classroom {
  id: string;
  name: string;
  code: string;
  studentCount: number;
}

interface Project {
  id: string;
  title: string;
  classroomId: string | null;
}

const props = defineProps<{
  classrooms: Classroom[];
  project: Project;
}>();

const emit = defineEmits<{
  "update:classroom": [classroomId: string | null];
}>();

const isOpen = ref(false);
const selectedClassroom = computed(
  () => props.classrooms.find((c) => c.id === props.project.classroomId) ?? null,
);

function selectClassroom(classroom: Classroom | null): void {
  emit("update:classroom", classroom?.id ?? null);
  isOpen.value = false;
}

function handleClickOutside(event: MouseEvent): void {
  const target = event.target as HTMLElement;
  if (!target.closest(".binder")) {
    isOpen.value = false;
  }
}

watch(isOpen, (val) => {
  if (val) {
    document.addEventListener("click", handleClickOutside);
  } else {
    document.removeEventListener("click", handleClickOutside);
  }
});
</script>

<template>
  <div class="binder">
    <button type="button" class="binder-trigger" @click="isOpen = !isOpen">
      <Link2 :size="14" />
      <span v-if="selectedClassroom" class="binder-selected">
        {{ selectedClassroom.name }}
      </span>
      <span v-else class="binder-placeholder">绑定班级</span>
      <ChevronDown :size="12" class="binder-caret" :class="{ 'is-open': isOpen }" />
    </button>

    <Transition name="menu">
      <div v-if="isOpen" class="binder-menu">
        <button
          type="button"
          class="binder-option"
          :class="{ 'is-selected': !selectedClassroom }"
          @click="selectClassroom(null)"
        >
          <span>不绑定</span>
          <Check v-if="!selectedClassroom" :size="14" class="binder-check" />
        </button>

        <div class="binder-divider" />

        <button
          v-for="classroom in classrooms"
          :key="classroom.id"
          type="button"
          class="binder-option"
          :class="{ 'is-selected': classroom.id === project.classroomId }"
          @click="selectClassroom(classroom)"
        >
          <div class="binder-option-info">
            <BookOpen :size="14" />
            <span>{{ classroom.name }}</span>
            <span class="binder-student-count">{{ classroom.studentCount }}人</span>
          </div>
          <Check v-if="classroom.id === project.classroomId" :size="14" class="binder-check" />
        </button>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.binder {
  position: relative;
}

.binder-trigger {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-pill);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.binder-trigger:hover {
  border-color: var(--color-border);
  color: var(--color-text);
}

.binder-selected {
  color: var(--color-text);
  font-weight: 500;
}

.binder-placeholder {
  color: var(--color-text-tertiary);
}

.binder-caret {
  transition: transform var(--duration-fast) var(--ease-out);
}

.binder-caret.is-open {
  transform: rotate(180deg);
}

.binder-menu {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  min-width: 200px;
  padding: 4px;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
  background: var(--color-surface);
  box-shadow: var(--shadow-lg);
  z-index: 50;
}

.binder-option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 8px 12px;
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 13px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.binder-option:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}

.binder-option.is-selected {
  background: var(--color-accent-soft);
  color: var(--color-accent);
}

.binder-option-info {
  display: flex;
  align-items: center;
  gap: 8px;
}

.binder-student-count {
  font-size: 11px;
  color: var(--color-text-tertiary);
}

.binder-check {
  color: var(--color-accent);
}

.binder-divider {
  height: 1px;
  margin: 4px 0;
  background: var(--color-border-subtle);
}

/* 菜单过渡 */
.menu-enter-active,
.menu-leave-active {
  transition: all var(--duration-fast) var(--ease-out);
}

.menu-enter-from,
.menu-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
