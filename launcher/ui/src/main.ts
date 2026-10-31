import { engine } from "animejs";
import { createApp } from "vue";
import App from "./App.vue";
import "./styles/main.css";

// 动画推进跟随浏览器刷新, 不受库内 240Hz 默认上限约束.
engine.fps = 1000;
createApp(App).mount("#root");
