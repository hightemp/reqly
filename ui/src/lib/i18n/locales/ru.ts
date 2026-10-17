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
  kv: {
    name: 'Имя',
    value: 'Значение',
    newName: 'Новое имя',
    newValue: 'Новое значение',
    description: 'Описание',
    enabled: 'Включено',
    deleteRow: 'Удалить строку',
    deleteAll: 'Удалить все',
    dragHandle: 'Перетащите для сортировки',
    bulkEdit: 'Текстом',
    tableEdit: 'Таблицей',
    showDescription: 'Описание',
    hideDescription: 'Скрыть описание',
    kind: 'Тип поля',
    kindText: 'Текст',
    kindFile: 'Файл',
    chooseFile: 'Выбрать файл…',
    inheritedFrom: 'Унаследовано от {source}',
  },
  tree: {
    delete: 'Удалить',
    deleteOneTitle: 'Удалить «{name}»?',
    deleteManyTitle: 'Удалить элементов: {count}?',
    deleteMessage: 'Элементы попадут в корзину, их можно будет восстановить.',
    deleted: {
      one: 'Удалён {count} элемент',
      few: 'Удалено {count} элемента',
      many: 'Удалено {count} элементов',
      other: 'Удалено {count} элемента',
    },
    undo: 'Отменить',
    curlCopied: {
      one: 'Скопирована {count} команда cURL',
      few: 'Скопировано {count} команды cURL',
      many: 'Скопировано {count} команд cURL',
      other: 'Скопировано {count} команды cURL',
    },
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
