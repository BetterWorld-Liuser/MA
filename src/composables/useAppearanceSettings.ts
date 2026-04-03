import { readonly, ref } from 'vue';

export type ThemeMode = 'dark' | 'light';

const STORAGE_KEY = 'ma.theme';
const DEFAULT_THEME: ThemeMode = 'dark';
const theme = ref<ThemeMode>(resolveInitialTheme());

export function initializeAppearanceTheme() {
  applyTheme(theme.value);
}

export function useAppearanceSettings() {
  function setTheme(nextTheme: ThemeMode) {
    if (theme.value === nextTheme) {
      return;
    }

    theme.value = nextTheme;
    persistTheme(nextTheme);
    applyTheme(nextTheme);
  }

  return {
    theme: readonly(theme),
    setTheme,
  };
}

function resolveInitialTheme(): ThemeMode {
  if (typeof window === 'undefined') {
    return DEFAULT_THEME;
  }

  const stored = window.localStorage.getItem(STORAGE_KEY);
  return isThemeMode(stored) ? stored : DEFAULT_THEME;
}

function persistTheme(nextTheme: ThemeMode) {
  if (typeof window === 'undefined') {
    return;
  }

  window.localStorage.setItem(STORAGE_KEY, nextTheme);
}

function applyTheme(nextTheme: ThemeMode) {
  if (typeof document === 'undefined') {
    return;
  }

  document.documentElement.dataset.theme = nextTheme;
  document.documentElement.style.colorScheme = nextTheme;
}

function isThemeMode(value: string | null): value is ThemeMode {
  return value === 'dark' || value === 'light';
}
