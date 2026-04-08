import { createApp } from 'vue';
import App from './App.vue';
import { initializeAppearanceTheme } from './composables/useAppearanceSettings';
import { debugChat } from './lib/chatDebug';
import './styles/vars.css';
import './styles/main.css';
import 'markstream-vue/index.css';

debugChat('main', 'module:init');

initializeAppearanceTheme();

window.addEventListener('beforeunload', () => {
  debugChat('main', 'window:beforeunload');
});

const app = createApp(App);
debugChat('main', 'app:created');
app.mount('#app');
debugChat('main', 'app:mounted');
