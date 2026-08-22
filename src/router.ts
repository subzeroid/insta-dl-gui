import { createRouter, createWebHistory } from "vue-router";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", redirect: "/download" },
    { path: "/onboarding", component: () => import("./views/OnboardingView.vue") },
    { path: "/download", component: () => import("./views/DownloadView.vue") },
    { path: "/queue", component: () => import("./views/QueueView.vue") },
    { path: "/settings", component: () => import("./views/SettingsView.vue") },
  ],
});
