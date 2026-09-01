import { createRouter, createWebHistory } from "vue-router";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", redirect: "/explore" },
    { path: "/onboarding", component: () => import("./views/OnboardingView.vue") },
    { path: "/download", component: () => import("./views/DownloadView.vue") },
    { path: "/explore", component: () => import("./views/ExplorerView.vue") },
    {
      path: "/explore/:username/:kind(followers|following)",
      component: () => import("./views/ProfileRelationshipsView.vue"),
      props: true,
    },
    { path: "/library", component: () => import("./views/LibraryView.vue") },
    { path: "/queue", component: () => import("./views/QueueView.vue") },
    { path: "/settings", component: () => import("./views/SettingsView.vue") },
  ],
});
