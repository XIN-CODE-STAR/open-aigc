import { createApp } from "vue";
import { createPinia } from "pinia";

import { router } from "./app/router";
import { usePreferencesStore } from "./app/stores/preferences";
import App from "./App.vue";
import "./styles.css";

const app = createApp(App);
const pinia = createPinia();

app.use(pinia);
app.use(router);

usePreferencesStore(pinia).initialize();
app.mount("#app");
