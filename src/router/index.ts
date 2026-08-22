import { createRouter, createWebHashHistory } from 'vue-router';

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', redirect: '/files' },
    { path: '/files', name: 'files', component: () => import('@/views/FileManager.vue'), meta: { title: '文件' } },
    { path: '/parse', name: 'parse', component: () => import('@/views/ParseView.vue'), meta: { title: 'UC 解析' } },
    { path: '/downloads', name: 'downloads', component: () => import('@/views/Downloads.vue'), meta: { title: '离线下载' } },
    { path: '/history', name: 'history', component: () => import('@/views/HistoryView.vue'), meta: { title: '历史记录' } },
    { path: '/settings', name: 'settings', component: () => import('@/views/Settings.vue'), meta: { title: '设置' } },
  ],
});

export default router;
