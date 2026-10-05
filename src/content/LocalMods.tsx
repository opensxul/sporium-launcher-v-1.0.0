import { useCallback, useEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { FilePlus2, Upload } from 'lucide-react';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import type { LocalContentPlan } from '../bindings/core';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';
import { DependencyReport } from './ContentDiagnostics';
import type { MessageKey } from '../i18n/messages';

export function LocalMods({
  id,
  disabled,
  area,
  onImported,
}: {
  id: string;
  disabled: boolean;
  area: RefObject<HTMLElement | null>;
  onImported: () => void;
}) {
  const { t, desktop } = useFoundation();
  const { snapshot } = useLibrary();
  const instance = snapshot.instances.find((item) => item.id === id);
  const [working, setWorking] = useState(false);
  const [hover, setHover] = useState(false);
  const [plan, setPlan] = useState<LocalContentPlan | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  const mounted = useRef(false);
  const token = useRef<string | null>(null);
  const inflight = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (token.current)
        void backend.finishLocalContent(token.current, false, true).catch(() => {});
    };
  }, []);
  const begin = useCallback(
    async (paths?: string[]) => {
      if (disabled || inflight.current || plan || !desktop) return;
      inflight.current = true;
      setWorking(true);
      setError(null);
      try {
        const value = paths
          ? await backend.localContentPlan(id, paths)
          : await backend.pickLocalContent(id);
        if (!mounted.current) {
          if (value) await backend.finishLocalContent(value.plan.token, false, true);
          return;
        }
        token.current = value?.plan.token ?? null;
        setPlan(value);
      } catch (reason) {
        if (mounted.current) setError(normalizeError(reason));
      } finally {
        inflight.current = false;
        if (mounted.current) setWorking(false);
      }
    },
    [id, disabled, plan, desktop],
  );
  useEffect(() => {
    if (!desktop) return;
    let active = true;
    const listening = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (!active) return;
      if (payload.type === 'leave') {
        setHover(false);
        return;
      }
      const bounds = area.current?.getBoundingClientRect();
      const x = payload.position.x / window.devicePixelRatio,
        y = payload.position.y / window.devicePixelRatio;
      const inside =
        !!bounds && x >= bounds.left && x <= bounds.right && y >= bounds.top && y <= bounds.bottom;
      const ready =
        inside && !disabled && !working && !plan && !document.querySelector('dialog[open]');
      setHover(ready && payload.type !== 'drop');
      if (payload.type === 'drop' && ready) void begin(payload.paths);
    });
    void listening.catch((reason) => {
      if (active) setError(normalizeError(reason));
    });
    return () => {
      active = false;
      void listening.then((unlisten) => unlisten()).catch(() => {});
    };
  }, [desktop, disabled, working, plan, area, begin]);
  async function finish(cancel: boolean) {
    if (!plan) return;
    setWorking(true);
    setError(null);
    try {
      await backend.finishLocalContent(
        plan.plan.token,
        plan.warnings.length > 0 && !cancel,
        cancel,
      );
      token.current = null;
      setPlan(null);
      if (!cancel) onImported();
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setWorking(false);
    }
  }
  return (
    <div className="local-mod-controls">
      <button
        className="button secondary"
        disabled={disabled || working || !desktop}
        onClick={() => void begin()}
      >
        <FilePlus2 size={17} />
        {t(working ? 'local.inspecting' : 'local.add')}
      </button>
      <span className="setting-hint">{t('local.dropHint')}</span>
      {hover && (
        <div className="local-drop-overlay" role="status">
          <Upload size={32} />
          {t('local.dropHere')}
        </div>
      )}
      {error && !plan && <ErrorNotice error={error} />}
      {plan && (
        <Modal title={t('local.preview')} busy={working} onClose={() => void finish(true)}>
          <p>
            {t('local.destination')} <strong>{instance?.name}</strong>
            <br />
            {instance?.minecraftVersion} · {instance?.loader} · {t('content.mod')}:{' '}
            {plan.plan.files.length}
          </p>
          <ul className="local-plan-files">
            {plan.plan.files.map((file) => (
              <li key={file.file.filename}>
                <strong>{file.title}</strong> · {file.version.version_number}
                <small>{file.provider === 'modrinth' ? 'Modrinth' : t('local.source')}</small>
                <details>
                  <summary>{t('content.fileDetails')}</summary>
                  <p>{file.file.filename}</p>
                  <p>{file.local?.loader || t('local.unknown')}</p>
                  <p>{file.local?.modIds.join(', ')}</p>
                  {!!file.local?.required.length && (
                    <p>
                      {t('local.required')}: {file.local.required.join(', ')}
                    </p>
                  )}
                </details>
              </li>
            ))}
          </ul>
          {plan.warnings.length > 0 && (
            <div className="content-warning">
              <p>{t('local.warning')}</p>
              <ul>
                {plan.warnings.map((code) => (
                  <li key={code}>{t(`local.warning.${code}` as MessageKey)}</li>
                ))}
              </ul>
            </div>
          )}
          {error && <ErrorNotice error={error} />}
          <DependencyReport
            report={plan.diagnostics}
            files={plan.plan.files.map((file) => ({
              directory: file.directory,
              filename: file.file.filename,
            }))}
          />
          <div className="modal-actions">
            {plan.plan.files.every((file) => file.provider === 'local') && (
              <button
                className="button secondary"
                disabled={working || disabled}
                onClick={() => {
                  setWorking(true);
                  setError(null);
                  void backend
                    .localDependencies(plan.plan.token)
                    .then((value) => {
                      if (mounted.current) setPlan(value);
                    })
                    .catch((reason) => {
                      if (mounted.current) setError(normalizeError(reason));
                    })
                    .finally(() => {
                      if (mounted.current) setWorking(false);
                    });
                }}
              >
                {t('dependencies.find')}
              </button>
            )}
            <button
              className="button secondary"
              disabled={working}
              onClick={() => void finish(true)}
            >
              {t('common.cancel')}
            </button>
            <button
              className="button primary"
              disabled={working || disabled}
              onClick={() => void finish(false)}
            >
              {t(plan.warnings.length ? 'local.installAnyway' : 'content.install')}
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
