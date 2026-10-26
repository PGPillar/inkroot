export type IconName = 'root' | 'rescue' | 'stock' | 'warning' | 'check' | 'backup' | 'info';

/** 24x24 viewBox glyph paths. root/rescue/stock are copied verbatim from the approved mockup. */
export const ICON_PATHS: Record<IconName, string> = {
  root: 'M12 2l3 6 6 1-4.5 4 1 6.5L12 16l-5.5 3.5 1-6.5L3 9l6-1z',
  rescue: 'M12 2a10 10 0 100 20 10 10 0 000-20zm1 5v6h-2V7zm0 8v2h-2v-2z',
  stock: 'M12 4V1L7 6l5 5V7a5 5 0 11-5 5H5a7 7 0 107-8z',
  warning: 'M12 2 1 21h22L12 2zm1 7v6h-2V9zm0 8v2h-2v-2z',
  check: 'M9 16.2 4.8 12l-1.4 1.4L9 19 21 7l-1.4-1.4z',
  backup: 'M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2z',
  info: 'M12 2a10 10 0 100 20 10 10 0 000-20zm1 15h-2v-6h2zm0-8h-2V7h2z',
};
