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
import BaseSelect from "./BaseSelect.vue";
import DirectoryField from "./DirectoryField.vue";

const selected = ref("");
const creating = ref(false);
const name = ref("");
const proton = ref("");
const confirming = ref(false);
const sourceId = ref<string | null>(null);
const sourceName = ref("");
const settings = computed(() => state.bootstrap?.settings);
const profiles = computed(() => settings.value?.runtimeEnvironments ?? []);
const active = computed(() => settings.value?.activeEnvironment ?? "");
const stable = computed(() => settings.value?.stableEnvironment ?? "");
const profile = computed(() =>
  profiles.value.find((entry) => entry.id === selected.value),
);
const options = computed(() => [
  { value: "", label: "原始环境" + suffix("") },
  ...profiles.value.map((entry) => ({
    value: entry.id,
    label: entry.name + suffix(entry.id),
  })),
]);
const removable = computed(
  () =>
    selected.value !== "" &&
    selected.value !== active.value &&
    selected.value !== stable.value,
);

function suffix(id: string) {
  return (
    (id === active.value ? " · 当前" : "") +
    (id === stable.value ? " · 稳定" : "")
  );
}
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

function openCreate() {
  sourceId.value = selected.value || null;
  sourceName.value = profile.value?.name ?? "原始环境";
  proton.value =
    profile.value?.proton.path ||
    form.protonPath ||
    settings.value?.protonPath ||
    settings.value?.originalProtonPin?.path ||
    "";
  name.value = "";
  creating.value = true;
}
async function create() {
  if (await createRuntimeEnvironment(name.value, proton.value, sourceId.value))
    creating.value = false;
}
async function discard() {
  if (await runtimeEnvironmentAction("discard", selected.value)) {
    selected.value = active.value;
    confirming.value = false;
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
    <div class="field-grid">
      <div class="field field--wide">
        <label for="runtime-environment">运行环境</label>
        <BaseSelect
          id="runtime-environment"
          v-model="selected"
          :options="options"
          :disabled="runtimeEnvironmentLocked"
        />
        <small class="field__hint" v-if="profile"
          >Proton: {{ profile.proton.path }}<br />游戏安装:
          {{ profile.installRoot }} ({{
            profile.distribution === "cn" ? "国服" : "国际服"
          }})</small
        >
        <small class="field__hint" v-else
          >原始环境的 prefix 和 Proton 在“安装与运行”中配置.</small
        >
      </div>
    </div>
    <div class="runtime-environments__actions">
      <button
        type="button"
        class="btn btn--ghost"
        :disabled="runtimeEnvironmentLocked || selected === active"
        @click="runtimeEnvironmentAction('switch', selected || null)"
      >
        切换到此环境
      </button>
      <button
        type="button"
        class="btn btn--ghost"
        :disabled="runtimeEnvironmentLocked || selected === stable"
        @click="runtimeEnvironmentAction('keep', selected || null)"
      >
        保留为稳定环境
      </button>
      <button
        type="button"
        class="btn btn--ghost"
        :disabled="runtimeEnvironmentLocked || !removable"
        @click="confirming = true"
      >
        删除实验环境
      </button>
      <button
        type="button"
        class="btn btn--ghost"
        :disabled="
          runtimeEnvironmentLocked ||
          !state.bootstrap ||
          (selected === active && !state.bootstrap.prefixExists)
        "
        @click="openCreate"
      >
        从所选环境创建副本
      </button>
    </div>
    <small class="field__hint"
      >当前 prefix: {{ state.bootstrap?.prefix || "尚未配置" }}</small
    >
    <div v-if="confirming" class="runtime-environments__confirmation">
      <p>确定删除“{{ profile?.name }}”的 prefix? 此操作无法撤销.</p>
      <div class="runtime-environments__actions">
        <button
          type="button"
          class="btn btn--ghost"
          :disabled="runtimeEnvironmentLocked"
          @click="discard"
        >
          确定删除
        </button>
        <button
          type="button"
          class="btn btn--link"
          :disabled="runtimeEnvironmentLocked"
          @click="confirming = false"
        >
          取消
        </button>
      </div>
    </div>
    <div v-if="creating" class="runtime-environments__create">
      <p class="runtime-environments__notice">
        源环境: {{ sourceName }}. 复制其 prefix 后使用所选 Proton. 映射到宿主的
        Windows 用户目录将在副本中使用独立空目录, 首次启动可能需要重新登录.
      </p>
      <div class="field-grid">
        <div class="field">
          <label for="runtime-environment-name">环境名称</label>
          <input
            id="runtime-environment-name"
            v-model="name"
            maxlength="64"
            :disabled="runtimeEnvironmentLocked"
            placeholder="例如 GE-Proton 新版本测试"
          />
        </div>
        <DirectoryField
          id="runtime-environment-proton"
          v-model="proton"
          label="实验 Proton 目录"
          :disabled="runtimeEnvironmentLocked"
        />
      </div>
      <div class="runtime-environments__actions">
        <button
          type="button"
          class="btn btn--primary"
          :disabled="runtimeEnvironmentLocked || !name.trim() || !proton.trim()"
          @click="create"
        >
          创建实验环境
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
    </div>
    <p v-if="runtimeEnvironmentBusy" role="status" aria-live="polite">
      正在处理运行环境, 请稍候.
    </p>
    <p v-if="runtimeEnvironmentError" role="alert">
      {{ runtimeEnvironmentError }}
    </p>
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
  gap: var(--space-8);
  margin: var(--space-16) 0;
}

.runtime-environments__create,
.runtime-environments__confirmation {
  margin-top: var(--space-16);
  padding-top: var(--space-16);
  border-top: 1px solid var(--line);
}

.field__hint {
  overflow-wrap: anywhere;
}
</style>
