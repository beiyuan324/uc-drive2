// Vitest 全局 setup：补齐 jsdom 缺失的浏览器 API
if (typeof window !== 'undefined' && !window.matchMedia) {
  window.matchMedia = ((query: string): MediaQueryList => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
}
