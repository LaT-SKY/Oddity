import { ref } from "vue";

// 拖动期间保留导航焦点，避免页面标题抢走手势。
export const navigationScrubbing = ref(false);
