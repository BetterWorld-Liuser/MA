import { createApp } from 'vue';
import App from './App.vue';
import { initializeAppearanceTheme } from './composables/useAppearanceSettings';
import './styles/vars.css';
import './styles/main.css';
import 'markstream-vue/index.css';

initializeAppearanceTheme();

createApp(App).mount('#app');
