import type { Instance } from '../bindings/core';
import { useFoundation } from '../app/context';
import { useGame, jobActive } from '../game/context';
import { useContent } from '../content/context';
import type { MessageKey } from '../i18n/messages';
export function InstanceStatus({ instance }: { instance: Instance }) {
  const { t } = useFoundation();
  const { state, starting } = useGame();
  const content = useContent();
  const running = state.sessions.some(
    (session) => session.instanceId === instance.id && session.running,
  );
  const job = state.job?.instanceId === instance.id ? state.job : null;
  const contentJob = content.job?.instanceId === instance.id ? content.job : null;
  let key: MessageKey =
    instance.status === 'installed' ? 'game.installed' : 'instance.notInstalled';
  let tone = instance.status === 'installed' ? 'ready' : 'neutral';
  if (job?.phase === 'failed' || contentJob?.phase === 'failed') {
    key = 'ui.failed';
    tone = 'error';
  }
  if (job && (jobActive(state) || starting)) {
    key = job.paused ? 'download.paused' : `job.${job.phase}`;
    tone = 'active';
  }
  if (contentJob && content.busy) {
    key = 'ui.preparing';
    tone = 'active';
  }
  if (running) {
    key = 'game.running';
    tone = 'active';
  }
  return <span className={`status-pill status-${tone}`}>{t(key)}</span>;
}
