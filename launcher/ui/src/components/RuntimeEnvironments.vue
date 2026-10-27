<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  state,
  form,
  runtimeEnvironmentBusy,
  runtimeEnvironmentError,
  runtimeEnvironmentLocked,
  createRuntimeEnvironment,
  runtimeEnvironmentAction,
} from "../core/controller";
import type { RuntimeEnvironment } from "../types/launcher";
import BaseModal from "./BaseModal.vue";
import BaseSelect from "./BaseSelect.vue";
import DirectoryField from "./DirectoryField.vue";

const selected = ref("");
const creating = ref(false);
const name = ref("");
const proton = ref("");
const confirming = ref(false);
const pendingRemoval = ref<RuntimeEnvironment | null>(null);
const sourceId = ref("");
const settings = computed(() => state.bootstrap?.settings);
const profiles = computed(() => settings.value?.runtimeEnvironments ?? []);
const active = computed(() => settings.value?.activeEnvironment ?? "");
const stable = computed(() => settings.value?.stableEnvironment ?? "");
const profile = computed(() =>
  profiles.value.find((entry) => entry.id === selected.value),
);
const source = computed(() =>
  profiles.value.find((entry) => entry.id === sourceId.value),
);
const options = computed(() =>
  profiles.value.map((entry) => ({
    value: entry.id,
    label:
      entry.name +
      (entry.id === active.value ? " · 当前" : "") +
      (entry.id === stable.value ? " · 稳定" : ""),
  })),
);
const sourceOptions = computed(() => [
  { value: "", label: "全新环境" },
  ...profiles.value.map((entry) => ({ value: entry.id, label: entry.name })),
]);
const removable = computed(
  () =>
    !!profile.value &&
    selected.value !== active.value &&
    selected.value !== stable.value,
);
watch(
  active,
  (value) => {
    selected.value = value;
  },
  { immediate: true },
);
watch(selected, () => {
  confirming.value = false;
  creating.value = false;
});
watch(sourceId, () => {
  proton.value = source.value?.proton.path ?? "";
});

function openCreate() {
  sourceId.value = profile.value?.id ?? "";
  proton.value = profile.value?.proton.path ?? "";
  name.value = "";
  runtimeEnvironmentError.value = "";
  creating.value = true;
}
async function create() {
  if (
    await createRuntimeEnvironment(
      name.value,
      proton.value,
      sourceId.value || null,
    )
  )
    creating.value = false;
}
function openDiscard() {
  pendingRemoval.value = profile.value ?? null;
  runtimeEnvironmentError.value = "";
  confirming.value = !!pendingRemoval.value;
}
async function discard() {
  const entry = pendingRemoval.value;
  if (entry && (await runtimeEnvironmentAction("discard", entry.id))) {
    confirming.value = false;
    selected.value = active.value;
  }
}
</script>

<template>
  <section
    class="settings-section"
    aria-labelledby="runtime-environments-title"
  >
    <h3 id="runtime-environments-title">Proton 运行环境</h3>
    <p class="runtime-environments__notice">
      各环境使用独立 prefix, 共享游戏安装.
      游戏更新、共享配置、宿主文件和云端状态不会随环境切换恢复.
    </p>
    <div v-if="profiles.length" class="field-grid">
      <div class="field field--wide">
        <label for="runtime-environment">运行环境</label>
        <BaseSelect
          id="runtime-environment"
          v-model="selected"
          :options="options"
          :disabled="runtimeEnvironmentLocked"
          placeholder="请选择运行环境"
        />
        <small v-if="profile" class="field__hint">
          Proton: {{ profile.proton.path }}<br />
          绑定的游戏目录: {{ profile.installRoot }} ({{
            profile.distribution === "cn" ? "国服" : "国际服"
          }})
        </small>
      </div>
    </div>
    <p v-else class="field__hint">请先创建 Proton 运行环境, 再启动游戏.</p>
    <div class="runtime-environments__actions">
      <button
        type="button"
        class="btn"
        :class="profiles.length ? 'btn--ghost' : 'btn--primary'"
        :disabled="runtimeEnvironmentLocked || !state.bootstrap"
        @click="openCreate"
      >
        <svg class="icon" aria-hidden="true"><use href="#i-plus" /></svg>
        {{ profiles.length ? "创建环境或副本" : "创建运行环境" }}
      </button>
      <template v-if="profiles.length">
        <button
          type="button"
          class="btn btn--link"
          :disabled="
            runtimeEnvironmentLocked || !profile || selected === active
          "
          @click="runtimeEnvironmentAction('switch', selected)"
        >
          切换到此环境
        </button>
        <button
          type="button"
          class="btn btn--link"
          :disabled="
            runtimeEnvironmentLocked || !profile || selected === stable
          "
          @click="runtimeEnvironmentAction('keep', selected)"
        >
          保留为稳定环境
        </button>
        <button
          type="button"
          class="btn btn--link runtime-environments__danger"
          :disabled="runtimeEnvironmentLocked || !removable"
          @click="openDiscard"
        >
          删除环境
        </button>
      </template>
    </div>
    <small v-if="active" class="field__hint"
      >当前 prefix: {{ state.bootstrap?.prefix || "不可用" }}</small
    >
    <BaseModal
      :open="creating"
      title="创建运行环境"
      :theme="form.theme"
      :reduce-motion="form.reduceMotion"
      @update:open="creating = $event"
    >
      <div class="field-grid">
        <div v-if="profiles.length" class="field field--wide">
          <label for="runtime-environment-source">创建来源</label>
          <BaseSelect
            id="runtime-environment-source"
            v-model="sourceId"
            :options="sourceOptions"
            :disabled="runtimeEnvironmentLocked"
          />
        </div>
        <div class="field field--wide">
          <label for="runtime-environment-name">环境名称</label>
          <input
            id="runtime-environment-name"
            v-model="name"
            maxlength="64"
            :disabled="runtimeEnvironmentLocked"
            placeholder="例如 GE-Proton 稳定版"
          />
        </div>
        <DirectoryField
          id="runtime-environment-proton"
          v-model="proton"
          label="Proton 目录"
          class="field--wide"
          :disabled="runtimeEnvironmentLocked"
        />
      </div>
      <p class="runtime-environments__notice" v-if="source">
        源环境: {{ source.name }}. 复制其 prefix 后使用所选 Proton. 映射到宿主的
        Windows 用户目录将在副本中使用独立空目录, 首次启动可能需要重新登录.
      </p>
      <p class="runtime-environments__notice" v-else>
        创建独立 prefix, 首次启动时由 Proton 初始化.
        首个环境会自动设为当前和稳定环境.
      </p>
      <p v-if="runtimeEnvironmentBusy" role="status" aria-live="polite">
        正在创建运行环境, 请稍候.
      </p>
      <p v-if="runtimeEnvironmentError" role="alert">
        {{ runtimeEnvironmentError }}
      </p>
      <div class="runtime-environments__actions">
        <button
          type="button"
          class="btn btn--primary"
          :disabled="runtimeEnvironmentLocked || !name.trim() || !proton.trim()"
          @click="create"
        >
          <svg class="icon" aria-hidden="true">
            <use :href="source ? '#i-copy' : '#i-plus'" />
          </svg>
          {{ source ? "创建副本" : "创建环境" }}
        </button>
        <button
          type="button"
          class="btn btn--link"
          :disabled="runtimeEnvironmentLocked"
          @click="creating = false"
        >
          取消创建
        </button>
      </div>
    </BaseModal>
    <p
      v-if="runtimeEnvironmentBusy && !creating && !confirming"
      role="status"
      aria-live="polite"
    >
      正在处理运行环境, 请稍候.
    </p>
    <p v-if="runtimeEnvironmentError && !creating && !confirming" role="alert">
      {{ runtimeEnvironmentError }}
    </p>
    <BaseModal
      :open="confirming"
      title="删除运行环境"
      :theme="form.theme"
      :reduce-motion="form.reduceMotion"
      @update:open="
        (value) => {
          if (!value) confirming = false;
        }
      "
    >
      <p>确定删除“{{ pendingRemoval?.name }}”的 prefix? 此操作无法撤销.</p>
      <p class="field__hint">游戏安装和其他环境不受影响.</p>
      <p v-if="runtimeEnvironmentError" role="alert">
        {{ runtimeEnvironmentError }}
      </p>
      <p v-if="runtimeEnvironmentBusy" role="status" aria-live="polite">
        正在删除运行环境, 请稍候.
      </p>
      <div class="runtime-environments__actions">
        <button
          type="button"
          class="btn btn--ghost runtime-environments__danger"
          :disabled="runtimeEnvironmentLocked"
          @click="discard"
        >
          <svg class="icon" aria-hidden="true"><use href="#i-trash" /></svg>
          确定删除
        </button>
        <button
          type="button"
          class="btn btn--link"
          :disabled="runtimeEnvironmentBusy"
          @click="confirming = false"
        >
          取消
        </button>
      </div>
    </BaseModal>
  </section>
</template>

<style scoped>
.runtime-environments__notice {
  margin: 0 0 var(--space-16);
  color: var(--muted);
  font-size: var(--text-sm);
  line-height: 1.7;
}

.runtime-environments__actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-24);
  margin: var(--space-16) 0;
}

.runtime-environments__danger,
.runtime-environments__danger .icon,
.runtime-environments__danger:hover:not(:disabled) {
  color: var(--danger);
}

.btn--ghost.runtime-environments__danger {
  background: var(--danger-fill);
}

.field__hint {
  overflow-wrap: anywhere;
}
</style>
