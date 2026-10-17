import type { Translation } from '../i18n.svelte';
import type { Messages } from './en';

export const ru: Translation<Messages> = {
  app: {
    name: 'Reqly',
    loading: 'Загрузка…',
    connectionLost: 'Соединение с сервером потеряно. Переподключение…',
  },
  common: {
    ok: 'ОК',
    cancel: 'Отмена',
    close: 'Закрыть',
  },
  window: {
    minimize: 'Свернуть',
    maximize: 'Развернуть',
    restore: 'Восстановить',
    close: 'Закрыть',
  },
  sidebar: {
    title: 'Коллекции',
    empty: 'Запросов пока нет',
    collapse: 'Скрыть боковую панель',
    expand: 'Показать боковую панель',
  },
  request: {
    empty: 'Выберите или создайте запрос',
  },
  response: {
    empty: 'Отправьте запрос, чтобы увидеть ответ',
  },
  settings: {
    theme: 'Тема',
    themeSystem: 'Системная',
    language: 'Язык',
    layout: 'Переключить расположение ответа',
  },
  languages: {
    en: 'English',
    ru: 'Русский',
  },
  errors: {
    unknown: 'Что-то пошло не так',
    'transport.network': 'Нет связи с сервером Reqly',
    'command.unknown': 'Неизвестная команда «{name}»',
  },
};
