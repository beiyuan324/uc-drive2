import type { GlobalThemeOverrides } from 'naive-ui';
import { darkTheme, dateZhCN, zhCN } from 'naive-ui';

/**
 * uc-drive2 设计 token（Design Taste Frontend 技能 · calm-premium 产品 UI）
 * - 单一 accent：Emerald #059669（悬停 #047857）
 * - 中性基底：Zinc（浅 #FAFAFA / #18181B，深 #18181B / #E4E4E7）
 * - 圆角体系：控件 8px、面板 12px
 * - 无渐变、无紫色、无玻璃拟态、无纯黑投影
 */

export const ACCENT = '#059669';
export const ACCENT_HOVER = '#047857';

const radius = { small: '6px', medium: '8px', large: '12px' };

export const lightOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: ACCENT,
    primaryColorHover: ACCENT_HOVER,
    primaryColorPressed: '#065F46',
    primaryColorSuppl: ACCENT,
    infoColor: '#2563EB',
    successColor: '#16A34A',
    warningColor: '#D97706',
    errorColor: '#DC2626',
    bodyColor: '#FAFAFA',
    cardColor: '#FFFFFF',
    modalColor: '#FFFFFF',
    popoverColor: '#FFFFFF',
    textColorBase: '#18181B',
    textColor1: '#18181B',
    textColor2: '#3F3F46',
    textColor3: '#71717A',
    borderColor: '#E4E4E7',
    dividerColor: '#F4F4F5',
    hoverColor: 'rgba(24, 24, 27, 0.04)',
    borderRadius: radius.medium,
    borderRadiusSmall: radius.small,
    fontFamily:
      '"Geist", "PingFang SC", "Microsoft YaHei", "Noto Sans SC", -apple-system, "Segoe UI", sans-serif',
    fontFamilyMono: '"Cascadia Code", "JetBrains Mono", Consolas, monospace',
    boxShadow1: '0 1px 3px rgba(24, 24, 27, 0.06)',
    boxShadow2: '0 4px 16px rgba(24, 24, 27, 0.08)',
    boxShadow3: '0 8px 32px rgba(24, 24, 27, 0.12)',
  },
  Button: { borderRadiusMedium: '8px', fontWeight: '500', heightMedium: '34px' },
  Input: { borderRadius: '8px', heightMedium: '34px' },
  Card: { borderRadius: '12px' },
  Dialog: { borderRadius: '12px' },
  Modal: { borderRadius: '12px' },
  Table: { borderRadius: '12px', thColor: '#FAFAFA', tdColorHover: 'rgba(24, 24, 27, 0.03)' },
  DataTable: { borderRadius: '12px', thColor: '#FAFAFA' },
  Tag: { borderRadius: '6px' },
  Progress: { railColor: '#E4E4E7' },
  Tooltip: { borderRadius: '8px' },
  Menu: { borderRadius: '8px' },
};

export const darkOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: '#10B981',
    primaryColorHover: '#34D399',
    primaryColorPressed: '#059669',
    primaryColorSuppl: '#10B981',
    infoColor: '#60A5FA',
    successColor: '#4ADE80',
    warningColor: '#FBBF24',
    errorColor: '#F87171',
    bodyColor: '#18181B',
    cardColor: '#1F1F23',
    modalColor: '#1F1F23',
    popoverColor: '#1F1F23',
    textColorBase: '#E4E4E7',
    textColor1: '#E4E4E7',
    textColor2: '#A1A1AA',
    textColor3: '#9CA3AF',  // AA 对比度：在 #18181B 上约 7.6:1
    borderColor: '#3F3F46',
    dividerColor: '#27272A',
    hoverColor: 'rgba(228, 228, 231, 0.06)',
    borderRadius: radius.medium,
    borderRadiusSmall: radius.small,
    fontFamily:
      '"Geist", "PingFang SC", "Microsoft YaHei", "Noto Sans SC", -apple-system, "Segoe UI", sans-serif',
    fontFamilyMono: '"Cascadia Code", "JetBrains Mono", Consolas, monospace',
    boxShadow1: '0 1px 3px rgba(0, 0, 0, 0.4)',
    boxShadow2: '0 4px 16px rgba(0, 0, 0, 0.45)',
    boxShadow3: '0 8px 32px rgba(0, 0, 0, 0.55)',
  },
  Button: { borderRadiusMedium: '8px', fontWeight: '500', heightMedium: '34px' },
  Input: { borderRadius: '8px', heightMedium: '34px' },
  Card: { borderRadius: '12px' },
  Dialog: { borderRadius: '12px' },
  Modal: { borderRadius: '12px' },
  Table: { borderRadius: '12px', thColor: '#1F1F23', tdColorHover: 'rgba(228, 228, 231, 0.04)' },
  DataTable: { borderRadius: '12px', thColor: '#1F1F23' },
  Tag: { borderRadius: '6px' },
  Progress: { railColor: '#3F3F46' },
  Tooltip: { borderRadius: '8px' },
  Menu: { borderRadius: '8px' },
};

export { darkTheme, zhCN, dateZhCN };
