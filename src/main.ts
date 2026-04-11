import { createApp } from 'vue';
import App from './App.vue';
import { initializeAppearanceTheme } from './composables/useAppearanceSettings';
import { debugChat } from './lib/chatDebug';
import { frontendDiagnosticLogger } from './lib/frontendDiagnosticLogger';
import './styles/vars.css';
import './styles/main.css';
import 'markstream-vue/index.css';

debugChat('main', 'module:init');
void frontendDiagnosticLogger.debug('main', 'module:init');

initializeAppearanceTheme();

window.addEventListener('beforeunload', () => {
  debugChat('main', 'window:beforeunload');
  void frontendDiagnosticLogger.info('main', 'window:beforeunload');
});

const app = createApp(App);
debugChat('main', 'app:created');
void frontendDiagnosticLogger.debug('main', 'app:created');
app.mount('#app');
debugChat('main', 'app:mounted');
void frontendDiagnosticLogger.info('main', 'app:mounted');
