import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { applyTheme } from "./lib/themes.svelte";

const app = mount(App, { target: document.getElementById("app")! });
// 启动画面和 index.html 里临时设置的底色交给界面的主题样式
document.getElementById("boot")?.remove();
applyTheme();

export default app;
