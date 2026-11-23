import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useFoundation } from '../app/context';
import { Toggle, ErrorNotice } from '../components/ui';
import type { WorldSettings as Options } from '../bindings/core';
import { normalizeError, type BackendError } from '../services/backend';

export function WorldSettings({
  value,
  onChange,
  disabled,
  section,
}: {
  value: Options;
  onChange: (v: Options) => void;
  disabled: boolean;
  section: string;
}) {
  const { data } = useFoundation();
  const w = (ru: string, en: string) => (data.settings.values.locale === 'ru-RU' ? ru : en);
  const set = (patch: Partial<Options>) => onChange({ ...value, ...patch });
  const toggle = (key: 'autoJoin' | 'particles' | 'parallax' | 'autoUpdates', label: string) => (
    <Toggle
      checked={value[key]}
      onChange={(v) => set({ [key]: v })}
      label={label}
      description=""
      disabled={disabled}
    />
  );
  if (section === 'minecraft')
    return (
      <div className="world-options form-fields">
        <label>
          {w('Адрес Sporium', 'Sporium address')}
          <input
            disabled={disabled}
            maxLength={253}
            placeholder={w(
              'Имя сервера или IP, при необходимости с портом',
              'Hostname or IP, optionally with a port',
            )}
            value={value.address}
            onChange={(e) => set({ address: e.target.value.trim() })}
          />
        </label>
        <label>
          {w('Версия Minecraft для Sporium', 'Sporium Minecraft version')}
          <input
            disabled={disabled}
            maxLength={64}
            placeholder={w('Определять по ответу сервера', 'Use server response')}
            value={value.version}
            onChange={(e) => set({ version: e.target.value.trim() })}
          />
        </label>
        <label>
          {w('Память Java, МиБ', 'Java memory, MiB')}
          <input
            disabled={disabled}
            type="number"
            min={0}
            max={32768}
            value={value.memoryMb}
            onChange={(e) => set({ memoryMb: Number(e.target.value) })}
          />
          <small>
            {w(
              '0 — автоматически. Вручную от 512 до 32768. Параметр сборки имеет приоритет.',
              '0 for automatic, or 512–32768. The instance setting takes priority.',
            )}
          </small>
        </label>
        {toggle('autoJoin', w('Подключаться к Sporium после запуска', 'Join Sporium after launch'))}
        <label>
          {w('После запуска Minecraft', 'After Minecraft starts')}
          <select
            disabled={disabled}
            value={value.afterLaunch}
            onChange={(e) => set({ afterLaunch: e.target.value as Options['afterLaunch'] })}
          >
            <option value="open">{w('Оставить окно открытым', 'Keep open')}</option>
            <option value="minimize">{w('Свернуть окно', 'Minimize window')}</option>
            <option value="tray">{w('Свернуть в трей', 'Hide to system tray')}</option>
          </select>
        </label>
      </div>
    );
  if (section === 'animations')
    return (
      <div className="world-options form-fields">
        {toggle('particles', w('Светящиеся споры', 'Glowing spores'))}
        {toggle('parallax', w('Движение фона за курсором', 'Pointer parallax'))}
        <label>
          {w('Интенсивность эффектов', 'Effect intensity')} · {value.intensity}%
          <input
            type="range"
            min="0"
            max="100"
            disabled={disabled}
            value={value.intensity}
            onChange={(e) => set({ intensity: Number(e.target.value) })}
          />
        </label>
      </div>
    );
  return (
    <div className="world-options form-fields">
      <label>
        {w('Источник новостей и галереи', 'News and gallery feed')}
        <input
          disabled={disabled}
          value={value.feed}
          maxLength={2048}
          placeholder="https://…"
          onChange={(e) => set({ feed: e.target.value.trim() })}
        />
        <small>
          {w(
            'HTTPS-адрес JSON с публикациями и снимками сервера.',
            'HTTPS address of the JSON feed for server posts and photos.',
          )}
        </small>
      </label>
      {toggle(
        'autoUpdates',
        w('Автоматически проверять обновления', 'Check for launcher updates automatically'),
      )}
    </div>
  );
}
type DesktopPrefs = { startup: boolean; acceleration: boolean; library: string | null };
export function DesktopOptions({ section }: { section: 'storage' | 'advanced' }) {
  const { desktop, data } = useFoundation();
  const w = (ru: string, en: string) => (data.settings.values.locale === 'ru-RU' ? ru : en);
  const [updateState, setUpdateState] = useState('');
  useEffect(() => {
    const done = (e: Event) => setUpdateState((e as CustomEvent<string>).detail);
    window.addEventListener('sporium-update-result', done);
    return () => window.removeEventListener('sporium-update-result', done);
  }, []);
  const [prefs, setPrefs] = useState<DesktopPrefs | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  useEffect(() => {
    if (desktop)
      void invoke<DesktopPrefs>('desktop_preferences')
        .then(setPrefs)
        .catch((e) => setError(normalizeError(e)));
  }, [desktop]);
  async function perform(command: string, args?: Record<string, unknown>) {
    setBusy(true);
    setError(null);
    try {
      await invoke(command, args);
      setPrefs(await invoke<DesktopPrefs>('desktop_preferences'));
    } catch (e) {
      setError(normalizeError(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="world-options">
      {section === 'advanced' ? (
        <>
          <div>
            <button
              className="button secondary"
              disabled={!desktop || updateState === 'checking'}
              onClick={() => {
                setUpdateState('checking');
                window.dispatchEvent(new Event('sporium-check-update'));
              }}
            >
              {w('Проверить обновления', 'Check for updates')}
            </button>
            <p role="status">
              {updateState === 'checking'
                ? w('Проверка…', 'Checking…')
                : updateState === 'current'
                  ? w('Установлена актуальная версия.', 'You are up to date.')
                  : updateState === 'error'
                    ? w(
                        'Не удалось получить сведения об обновлении.',
                        'Update information is unavailable.',
                      )
                    : ''}
            </p>
          </div>
          <Toggle
            label={w('Запуск вместе с Windows', 'Start with Windows')}
            description=""
            checked={!!prefs?.startup}
            disabled={!prefs || busy}
            onChange={(enabled) => void perform('set_startup', { enabled })}
          />
          <Toggle
            label={w('Аппаратное ускорение', 'Hardware acceleration')}
            description={w('Применится после перезапуска.', 'Takes effect after restart.')}
            checked={prefs?.acceleration ?? true}
            disabled={!prefs || busy}
            onChange={(enabled) => void perform('set_acceleration', { enabled })}
          />
          <button
            className="button secondary"
            disabled={!desktop || busy}
            onClick={() => void perform('hide_to_tray')}
          >
            {w('Свернуть в системный трей', 'Hide to system tray')}
          </button>
        </>
      ) : (
        <>
          <h3>{w('Расположение библиотеки', 'Library location')}</h3>
          <p>
            {w(
              'Выберите пустую папку или библиотеку Sporium. При следующем запуске откроются сборки, профили и настройки выбранной папки.',
              'Choose an empty folder or an existing Sporium library. Its instances, profiles and settings open on the next launch.',
            )}
          </p>
          {prefs?.library && <code className="data-path">{prefs.library}</code>}
          <div className="instance-actions">
            <button
              className="button secondary"
              disabled={!prefs || busy}
              onClick={() => void perform('select_library')}
            >
              {w('Выбрать папку', 'Choose folder')}
            </button>
            {prefs?.library && (
              <button
                className="button secondary"
                disabled={busy}
                onClick={() => void perform('reset_library')}
              >
                {w('Вернуть стандартную', 'Use default')}
              </button>
            )}
          </div>
        </>
      )}
      {error && <ErrorNotice error={error} />}
    </div>
  );
}
