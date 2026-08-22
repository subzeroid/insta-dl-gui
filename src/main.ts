import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import { router } from "./router";
import { installTauriMock } from "./lib/mock";
import "./style.css";

if (new URLSearchParams(window.location.search).has("mock")) {
  installTauriMock();
}

createApp(App).use(createPinia()).use(router).mount("#app");
