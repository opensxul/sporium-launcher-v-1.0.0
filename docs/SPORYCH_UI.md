# Аудит интерфейса Sporych

База аудита — Sporium 2.0.0, ветка feature/sporium-launcher-2. Интерфейс использует React, TypeScript, HashRouter и CSS. Tauri связывает экранные действия с Rust через `src/services/backend.ts` и команды, зарегистрированные в `src-tauri/src/lib.rs`. Библиотека, профили и параметры хранятся в существующем каталоге приложения. Обновлятор старых клиентов ждёт подписанный `latest.json` и NSIS-установщик из GitHub Releases.

| Экран | Данные, действия и состояния |
| --- | --- |
| Главная | Статус сервера, новости, выбор Vanilla-сборки, запуск и повторная попытка. Состояния сети, прогресс, запущенная игра, код выхода. |
| Библиотека и сборки | Поиск, фильтры, сортировка, избранное, импорт и создание, открытие папки. Категории Vanilla, Fabric, Forge, NeoForge, Studio и Managed. |
| Страница сборки | Обзор, моды, ресурспаки, шейдеры, миры, датапаки, настройки, история и журналы. Установка, проверка и запуск, настройка Java и памяти, управление контентом и резервными копиями. |
| Папки | Список, просмотр, фильтры, создание, переименование и удаление коллекций. |
| Каталог | Поиск и фильтры Modrinth, сведения о проекте, выбор версии, установка, совместимость. Каталог доступен отдельным экраном и из сборки. |
| Новости и мир | Конфигурируемый источник, ошибки сети, пустые состояния, диалоги статьи и фотографии. |
| Профиль | Просмотр по нику, локальные аккаунты, создание, выбор, переименование, удаление и браузерный вход Microsoft. В доступном коде есть холст предпросмотра, получение игрового скина по нику и настройка модуля скинов. Загрузка пользовательского PNG, 3D-редактор и ручной выбор модели относятся к отдельной функциональной задаче. |
| Настройки | Аккаунты, Minecraft, Java, каталог, загрузки, хранилище, оформление, анимации, дополнительные настройки, версия приложения. Сохранение, конфликт, ошибки, предупреждение при уходе. |
| Загрузки | Очередь и прогресс с фактическими данными, пауза, отмена, повтор, скорость, подтверждение очистки кеша. |
| Диалоги | Формы сборок, иконки, папки, управление профилями, остановка игры, импорт и экспорт, каталог и установка контента, диагностика, зависимости, восстановление, планы Studio, фото и новости, подписанное обновление и несохранённые настройки. Общий компонент — `Modal`. |
| Служебные состояния | Запуск Tauri, ошибка и повтор загрузки страницы, отсутствующий маршрут, резервная копия, уведомления выполнения, режим предпросмотра. |

В текущем UI есть несколько каскадов палитры, скруглённые поверхности разной формы, линейные иконки и профиль сверху. Поиск библиотеки, сборок и каталога находится внутри соответствующих маршрутов. Компоненты кнопок, полей, вкладок и карточек имеют общие CSS-селекторы. Существующие обработчики формы и диалога определяют фокус, блокировку и возврат на исходный элемент.

Порядок замены: токены и локальные шрифты → каркас и навигация → главная и карточки → общие формы, вкладки, диалоги и обратная связь → регрессия экранов и проверка обновления. Поиск в верхней панели передаёт запрос в существующие представления. Идентификатор, каталог установки, профили, IPC, ключ обновлений и адрес GitHub Releases сохраняются.

## Иллюстрации интерфейса

Файлы в `public/branding/scenery/` созданы в ImageGen по мотивам референса. Баннер и фоны карточек иллюстрируют вымышленные миры. Иконки сборок приходят из Modrinth, а галерея сервера получает скриншоты из настроенного источника.

<details>
<summary>Промпты для иллюстраций</summary>

**castle-sunset.webp**

> Create a single finished game-world landscape as a background asset for Sporych Launcher, panoramic ultra-wide framing about 3.6:1. Minecraft-inspired in-game screenshot style, convincing large block geometry and crisp pixel textures, handsome stone medieval castle with square crenellated towers on a green hill at the FAR RIGHT, a winding river valley with low hills, blocky forests and a lantern by the path. Big golden square sun at the horizon on the right side, peach and purple stepped clouds, saturated sunset highlights. Right half bright and scenic. Left forty percent consists of a shaded forest with subdued leaves and dark soil so legible launcher HTML headings can be placed over it. Arrange the horizon in the middle, all key features in the middle horizontal band for a responsive cover crop. Warm and adventurous, slightly nostalgic Minecraft shader screenshot with handcrafted pixel art texture, no interface, no captions, no text, no logo, no overlays, no watermark. The castle and sunset are entirely original art.

**moss-stone.webp**

> Generate a narrow tall vertical bitmap texture used behind the left sidebar of a Minecraft-inspired desktop launcher. Aspect ratio 1:4 if supported, otherwise a tall portrait with an equally important narrow center crop. Very dark graphite and black blocky dark stone tiles, recessed small square pixel patterns, close values across the center for clear white text and icons overlaid by software. A few deep subdued moss green pixel vines creep from the upper left corner and lower right corner. Stone made of layered block textures, crisp limited color pixel art with a low-contrast material feel. The long central part must be visually quiet and almost flat dark. Strong silhouette in the uppermost and lowermost corners only, no frames or buttons, no UI, no letters, no logos, no watermark.

**fly-agaric.webp**

> Create a single original isolated 3D-inspired Minecraft pixel art fly agaric mushroom icon with a scarlet cubic stepped cap shaped like a broad red mushroom and cream square speckles, thick short textured cream stem, standing on a small grassy dirt cube. Three-quarter front view, with clear crisp pixel grid and simple graphic silhouette, light from upper left, restrained warm colors, tiny detailed block surface, premium voxel game launcher emblem. Center it, leave 8% transparent margin all around, entire sprite including cube visible and no separate objects, no glow outside, no lettering, no logo text, no watermark. Genuinely transparent background. Legibility as small as 64px, a red mushroom is the unmistakable primary element.

**autumn-cottage.webp, amethyst-islands.webp, medieval-village.webp**

> Create a 1:1 square production-ready sprite sheet made of THREE equally tall horizontal panoramic pixel-voxel game landscapes stacked vertically, edge-to-edge with absolutely no borders, no margins, no text, no labels, no symbols, no watermark. Each of the 3 separate panoramic rows is an illustration for a modpack card header in a Minecraft-inspired desktop launcher. Layout strict: row 1 occupies precisely top 33.33%, row 2 exactly middle 33.33%, row 3 exactly bottom 33.33%. Each is a high quality game-world landscape with one clear scene, close attention to coherent cubical block geometry and crunchy pixel textures, pixelated lighting. TOP: maple forest and a cozy timber cottage with glowing windows by a river on a warm peach sunset. MIDDLE: dark fantasy purple block cliffs and floating islands with amethyst-lit square towers and purple mist, no huge creature that eclipses scenery. BOTTOM: a medieval walled stone-and-timber village in a verdant valley framed by cube trees in crisp late-morning blue sunlight. Attractive, original imagery with clean separation between the three rows and completely different color and lighting in each. All important content is contained in the central band of each separate panel, suitable for crop. No interface elements, no buttons, no type, no text, no screenshots of any existing branded product. The three rows meet each other flush with a sharp horizontal transition.

</details>
