<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { fileRequest, type FileListing } from "../core/filePicker";
import { invoke } from "../core/api";
import { form } from "../core/controller";
import BaseModal from "./BaseModal.vue";
import BaseButton from "./BaseButton.vue";
import BaseInput from "./BaseInput.vue";
import BaseSwitch from "./BaseSwitch.vue";
const listing = ref<FileListing | null>(null);
const path = ref("");
const name = ref("");
const hidden = ref(false);
const error = ref("");
const busy = ref(false);
const overwrite = ref(false);
const entries = computed(
  () =>
    listing.value?.entries.filter(
      (entry) =>
        (hidden.value || !entry.name.startsWith(".")) &&
        (entry.directory ||
          (!fileRequest.value?.directory &&
            (!fileRequest.value?.extensions.length ||
              fileRequest.value.extensions.some((extension) =>
                entry.name
                  .toLowerCase()
                  .endsWith(`.${extension.toLowerCase()}`),
              )))),
    ) ?? [],
);
function close(value: string | null = null) {
  const request = fileRequest.value;
  fileRequest.value = null;
  request?.resolve(value);
}
async function browse(destination?: string) {
  busy.value = true;
  error.value = "";
  overwrite.value = false;
  const result = await invoke("browse_files", {
    path: destination ?? null,
  }).catch((cause) => {
    error.value = String(cause);
    return null;
  });
  if (result && fileRequest.value) {
    listing.value = result;
    path.value = result.path;
  }
  busy.value = false;
}
watch(fileRequest, (request) => {
  if (!request) return;
  listing.value = null;
  name.value = "";
  hidden.value = false;
  const initial = request.defaultPath ?? "";
  if (request.save) name.value = initial.split("/").at(-1) ?? "";
  const directory = request.save
    ? initial.slice(0, initial.lastIndexOf("/") + 1)
    : initial;
  void browse(directory || undefined);
});
function saveFile() {
  if (!listing.value) return;
  const filename = name.value.trim();
  if (
    !filename ||
    filename === "." ||
    filename === ".." ||
    filename.includes("/")
  ) {
    error.value = "请输入有效文件名.";
    return;
  }
  const existing = listing.value.entries.find(
    (entry) => entry.name === filename,
  );
  if (existing?.directory) {
    error.value = "该名称已被文件夹使用.";
    return;
  }
  if (existing && !overwrite.value) {
    overwrite.value = true;
    return;
  }
  close(`${listing.value.path.replace(/\/$/, "")}/${filename}`);
}
onBeforeUnmount(() => close());
</script>
<template>
  <BaseModal
    :open="!!fileRequest"
    :title="fileRequest?.title ?? ''"
    :theme="form.theme"
    :reduce-motion="form.reduceMotion"
    @update:open="!$event && close()"
  >
    <div class="pad-files">
      <div class="pad-files__path">
        <BaseInput
          id="pad-file-path"
          v-model="path"
          aria-label="文件夹路径"
        /><BaseButton :busy="busy" @click="browse(path)">前往</BaseButton>
      </div>
      <div class="pad-files__toolbar">
        <BaseButton
          :disabled="!listing?.parent || busy"
          @click="browse(listing?.parent ?? undefined)"
          >上一级</BaseButton
        ><BaseButton :disabled="busy" @click="browse(listing?.home)"
          >主目录</BaseButton
        ><BaseSwitch
          id="pad-files-hidden"
          v-model="hidden"
          label="显示隐藏文件"
        /><label for="pad-files-hidden">隐藏文件</label>
      </div>
      <div class="pad-files__list" aria-label="文件列表">
        <BaseButton
          v-for="entry in entries"
          :key="entry.path"
          :icon="entry.directory ? 'i-folder-simple' : 'i-download-simple'"
          :disabled="busy"
          @click="
            entry.directory
              ? browse(entry.path)
              : fileRequest?.save
                ? ((name = entry.name), (overwrite = false))
                : close(entry.path)
          "
          >{{ entry.name }}</BaseButton
        >
        <p v-if="!busy && !entries.length" class="field__hint">
          当前文件夹没有可选项目.
        </p>
      </div>
      <div v-if="fileRequest?.save" class="field">
        <label for="pad-file-name">文件名</label
        ><BaseInput
          id="pad-file-name"
          v-model="name"
          @input="overwrite = false"
        />
      </div>
      <p v-if="error" role="alert" class="field__hint">{{ error }}</p>
      <p v-if="overwrite" role="alert" class="field__hint">
        同名文件已存在, 再次确认将覆盖.
      </p>
      <div class="pad-files__toolbar">
        <BaseButton @click="close()">取消</BaseButton
        ><BaseButton
          v-if="fileRequest?.directory"
          variant="primary"
          :disabled="!listing || busy"
          @click="close(listing?.path)"
          >选择此文件夹</BaseButton
        ><BaseButton
          v-if="fileRequest?.save"
          variant="primary"
          :disabled="!listing || busy"
          @click="saveFile"
          >{{ overwrite ? "覆盖文件" : "保存" }}</BaseButton
        >
      </div>
    </div>
  </BaseModal>
</template>
