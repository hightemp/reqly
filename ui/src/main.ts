import { mount } from 'svelte';
import App from './App.svelte';
import { applyTheme } from '$lib/themes/theme';
import { dark, light } from '$lib/themes/builtin';
import './styles/global.css';
import { handleCallbackPage } from '$lib/platform/oauth2';
import { onTextContextMenu } from '$lib/ui/textMenu';
import { appTitle } from '$lib/version';

// Paint the right background before services load to avoid a white flash.
applyTheme(matchMedia('(prefers-color-scheme: dark)').matches ? dark : light);

// The browser context menu is replaced by custom menus; text gets its own.
const coarse = matchMedia('(pointer: coarse)');
document.addEventListener('contextmenu', (e) => onTextContextMenu(e, coarse.matches));

// Inside the OAuth 2.0 sign-in pop-up: hand the result back and stop.
const isCallback = handleCallbackPage();

if (!isCallback) document.title = appTitle();

export default isCallback ? undefined : mount(App, { target: document.getElementById('app')! });
