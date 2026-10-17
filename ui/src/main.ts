import { mount } from 'svelte';
import App from './App.svelte';
import { applyTheme } from '$lib/themes/theme';
import { dark, light } from '$lib/themes/builtin';
import './styles/global.css';
import { handleCallbackPage } from '$lib/platform/oauth2';

// Paint the right background before services load to avoid a white flash.
applyTheme(matchMedia('(prefers-color-scheme: dark)').matches ? dark : light);

// The browser context menu is replaced by custom menus; keep it in text fields.
document.addEventListener('contextmenu', (e) => {
  const target = e.target as HTMLElement | null;
  if (!target?.closest('input, textarea, [contenteditable="true"]')) e.preventDefault();
});

// Inside the OAuth 2.0 sign-in pop-up: hand the result back and stop.
const isCallback = handleCallbackPage();

export default isCallback ? undefined : mount(App, { target: document.getElementById('app')! });
