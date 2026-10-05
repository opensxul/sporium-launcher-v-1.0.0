import { useEffect, useState } from 'react';
import type { ContentUpdate, ContentUpdatePlan, InstalledContent } from '../bindings/core';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';
import { useFoundation } from '../app/context';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { useContent } from './context';
import type { MessageKey } from '../i18n/messages';

export function ContentUpdates({
  id,
  records,
  initial,
  onClose,
}: {
  id: string;
  records: InstalledContent[];
  initial: string[] | null;
  onClose: () => void;
}) {
  const { t } = useFoundation();
  const { install, busy, error: installError } = useContent();
  const [updates, setUpdates] = useState<ContentUpdate[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [working, setWorking] = useState(false);
  const [checking, setChecking] = useState(true);
  const [error, setError] = useState<BackendError | null>(null);
  const [plan, setPlan] = useState<ContentUpdatePlan | null>(null);
  useEffect(() => {
    let active = true;
    void backend
      .contentUpdates(id)
      .then((items) => {
        if (!active) return;
        setUpdates(items);
        setSelected(
          items
            .filter((u) => u.status === 'available' && (!initial || initial.includes(u.projectId)))
            .map((u) => u.projectId),
        );
      })
      .catch((reason) => {
        if (active) setError(normalizeError(reason));
      })
      .finally(() => {
        if (active) setChecking(false);
      });
    return () => {
      active = false;
    };
  }, [id, initial]);
  const locked = working || busy || checking;
  async function policy(item: ContentUpdate, action: 'pin' | 'ignore') {
    setWorking(true);
    setError(null);
    const next = {
      ...item.policy,
      ...(action === 'pin'
        ? { pinned: !item.policy.pinned }
        : { ignoredVersion: item.policy.ignoredVersion ? null : (item.candidate?.id ?? null) }),
    };
    try {
      await backend.contentUpdatePolicy(id, next);
      setUpdates(await backend.contentUpdates(id));
      setSelected((old) => old.filter((project) => project !== item.projectId));
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setWorking(false);
    }
  }
  async function preview() {
    setWorking(true);
    setError(null);
    try {
      const files = records
        .filter((r) => selected.includes(r.record.projectId))
        .map(({ record }) => ({
          directory: record.directory,
          filename: record.file.filename,
          sha512: record.file.hashes.sha512,
        }));
      setPlan(await backend.contentUpdatePlan({ instanceId: id, files }));
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setWorking(false);
    }
  }
  return (
    <Modal title={t('updates.title')} onClose={onClose} busy={working || busy}>
      <p className="setting-hint">
        {t(plan ? 'updates.snapshotHint' : 'updates.compatibilityHint')}
      </p>
      {error && <ErrorNotice error={error} />}
      {installError && <ErrorNotice error={installError} />}
      {checking && <p role="status">{t('updates.checking')}</p>}
      {plan ? (
        <ul className="update-plan">
          {plan.plan.files.map((record) => {
            const before = plan.previous.find((r) => r.projectId === record.projectId);
            return (
              <li key={`${record.directory}/${record.file.filename}`}>
                <strong>{record.title}</strong>
                <span>
                  {before
                    ? `${before.version.version_number} → ${record.version.version_number}`
                    : `+ ${record.version.version_number}`}
                </span>
                {record.dependency && <small>{t('content.required')}</small>}
                {record.directory === 'mods_disabled' && (
                  <small>{t('content.status.disabled')}</small>
                )}
              </li>
            );
          })}
        </ul>
      ) : (
        <>
          <button
            className="button secondary"
            disabled={locked}
            onClick={() =>
              setSelected(updates.filter((u) => u.status === 'available').map((u) => u.projectId))
            }
          >
            {t('updates.selectAll')}
          </button>
          <ul className="content-updates">
            {updates.map((item) => (
              <li key={item.projectId}>
                <label>
                  <input
                    type="checkbox"
                    aria-label={`${t('content.select')}: ${item.title}`}
                    disabled={locked || item.status !== 'available'}
                    checked={selected.includes(item.projectId)}
                    onChange={(e) =>
                      setSelected((old) =>
                        e.target.checked
                          ? [...old, item.projectId]
                          : old.filter((p) => p !== item.projectId),
                      )
                    }
                  />
                  <strong>{item.title}</strong>
                </label>
                <span>
                  {
                    records.find((r) => r.record.projectId === item.projectId)?.record.version
                      .version_number
                  }
                  {item.candidate && ` → ${item.candidate.version_number}`}
                </span>
                <small>{t(`updates.status.${item.status}` as MessageKey)}</small>
                {item.status !== 'local' && (
                  <div className="update-policy">
                    <button
                      className="button secondary"
                      aria-pressed={item.policy.pinned}
                      disabled={locked}
                      onClick={() => void policy(item, 'pin')}
                    >
                      {t(item.policy.pinned ? 'updates.unpin' : 'updates.pin')}
                    </button>
                    {(item.candidate || item.policy.ignoredVersion) && (
                      <button
                        className="button secondary"
                        disabled={locked || item.policy.pinned}
                        onClick={() => void policy(item, 'ignore')}
                      >
                        {t(item.policy.ignoredVersion ? 'updates.clearIgnore' : 'updates.ignore')}
                      </button>
                    )}
                  </div>
                )}
              </li>
            ))}
          </ul>
          {!checking && updates.length === 0 && <p>{t('updates.empty')}</p>}
        </>
      )}
      <div className="modal-actions">
        <button
          className="button secondary"
          disabled={working || busy}
          onClick={
            plan
              ? () => {
                  setPlan(null);
                  setError(null);
                }
              : onClose
          }
        >
          {t(plan ? 'updates.back' : 'common.cancel')}
        </button>
        <button
          className="button primary"
          disabled={locked || (!plan && selected.length === 0)}
          onClick={() => {
            if (plan) {
              void install(plan.plan.token).then((ok) => {
                if (ok) onClose();
              });
            } else {
              void preview();
            }
          }}
        >
          {t(plan ? 'updates.apply' : 'updates.preview')}
        </button>
      </div>
    </Modal>
  );
}
