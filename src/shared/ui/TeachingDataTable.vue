<script setup lang="ts" generic="TData">
import { ref } from "vue";
import { ArrowDown, ArrowUp, ArrowUpDown } from "@lucide/vue";
import {
  FlexRender,
  getCoreRowModel,
  getSortedRowModel,
  useVueTable,
  type ColumnDef,
  type SortingState,
} from "@tanstack/vue-table";

const props = defineProps<{
  columns: ColumnDef<TData>[];
  data: TData[];
  emptyText: string;
  getRowId: (row: TData) => string;
  rowLabel: (row: TData) => string;
  selectedId?: string;
}>();

const emit = defineEmits<{
  select: [row: TData];
}>();

const sorting = ref<SortingState>([]);
const table = useVueTable({
  get data() {
    return props.data;
  },
  get columns() {
    return props.columns;
  },
  getRowId: props.getRowId,
  getCoreRowModel: getCoreRowModel(),
  getSortedRowModel: getSortedRowModel(),
  state: {
    get sorting() {
      return sorting.value;
    },
  },
  onSortingChange: (updater) => {
    sorting.value = typeof updater === "function" ? updater(sorting.value) : updater;
  },
});
</script>

<template>
  <div class="data-table-scroll">
    <table class="data-table">
      <thead>
        <tr v-for="headerGroup in table.getHeaderGroups()" :key="headerGroup.id">
          <th
            v-for="header in headerGroup.headers"
            :key="header.id"
            :colspan="header.colSpan"
            :style="{
              width: header.column.columnDef.size ? `${header.column.getSize()}px` : undefined,
            }"
          >
            <template v-if="!header.isPlaceholder">
              <button
                v-if="header.column.getCanSort()"
                class="sort-button"
                type="button"
                :aria-label="`按${String(header.column.columnDef.header ?? '')}排序`"
                @click="header.column.getToggleSortingHandler()?.($event)"
              >
                <FlexRender :render="header.column.columnDef.header" :props="header.getContext()" />
                <ArrowUp
                  v-if="header.column.getIsSorted() === 'asc'"
                  :size="14"
                  aria-hidden="true"
                />
                <ArrowDown
                  v-else-if="header.column.getIsSorted() === 'desc'"
                  :size="14"
                  aria-hidden="true"
                />
                <ArrowUpDown v-else :size="14" aria-hidden="true" />
              </button>
              <FlexRender
                v-else
                :render="header.column.columnDef.header"
                :props="header.getContext()"
              />
            </template>
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-if="table.getRowModel().rows.length === 0">
          <td class="empty-cell" :colspan="Math.max(columns.length, 1)">{{ emptyText }}</td>
        </tr>
        <tr
          v-for="row in table.getRowModel().rows"
          v-else
          :key="row.id"
          class="data-row"
          :class="{ 'is-selected': selectedId === row.id }"
          tabindex="0"
          :aria-label="rowLabel(row.original)"
          :aria-selected="selectedId === row.id"
          @click="emit('select', row.original)"
          @keydown.enter="emit('select', row.original)"
          @keydown.space.prevent="emit('select', row.original)"
        >
          <td
            v-for="cell in row.getVisibleCells()"
            :key="cell.id"
            :style="{
              width: cell.column.columnDef.size ? `${cell.column.getSize()}px` : undefined,
            }"
          >
            <FlexRender :render="cell.column.columnDef.cell" :props="cell.getContext()" />
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.data-table-scroll {
  min-width: 0;
  overflow: auto;
  border: 1px solid var(--color-border-subtle);
  border-radius: var(--radius-surface);
}

.data-table {
  width: 100%;
  min-width: 640px;
  border-collapse: collapse;
  table-layout: fixed;
  font-size: 13px;
  line-height: 20px;
}

th,
td {
  height: 42px;
  padding: 0 var(--space-3);
  overflow: hidden;
  text-align: left;
  text-overflow: ellipsis;
  white-space: nowrap;
  border-bottom: 1px solid var(--color-border-subtle);
}

th {
  height: 36px;
  color: var(--color-text-secondary);
  background: var(--color-surface-subtle);
  font-size: 12px;
  font-weight: 600;
}

tbody tr:last-child td {
  border-bottom: 0;
}

.sort-button {
  display: inline-flex;
  width: 100%;
  height: 32px;
  padding: 0;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-1);
  color: inherit;
  border: 0;
  background: transparent;
  font: inherit;
  font-weight: inherit;
  cursor: pointer;
}

.data-row {
  cursor: pointer;
}

.data-row:hover {
  background: var(--color-surface-hover);
}

.data-row.is-selected {
  background: var(--color-surface-selected);
}

.data-row:focus-visible {
  position: relative;
  outline: 2px solid var(--color-focus);
  outline-offset: -2px;
}

.empty-cell {
  height: 120px;
  color: var(--color-text-secondary);
  text-align: center;
  white-space: normal;
}
</style>
