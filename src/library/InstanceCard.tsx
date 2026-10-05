import { InstanceIcon } from './InstanceIcon';
import { InstanceStatus } from './InstanceStatus';
import { ArrowUpRight } from 'lucide-react';
import { Link } from 'react-router-dom';
import type { Instance } from '../bindings/core';
import { useFoundation } from '../app/context';
import { useLibrary } from './context';
import { loaderNames } from './view-model';

export function InstanceCard({ instance }: { instance: Instance }) {
  const { t } = useFoundation();
  const { busy, snapshot } = useLibrary();
  const collection = snapshot.collections.find((item) => item.id === instance.collectionId);
  return (
    <Link
      className="instance-card"
      to={`/instance/${instance.id}`}
      draggable={!busy}
      onDragStart={(event) => {
        event.dataTransfer.setData('application/x-sporium-instance', instance.id);
        event.dataTransfer.effectAllowed = 'move';
      }}
    >
      <div className="instance-card-top">
        <InstanceIcon instance={instance} />
        <ArrowUpRight size={17} />
      </div>
      <h3>{instance.name}</h3>
      <p>
        {loaderNames[instance.loader]} · {instance.minecraftVersion}
      </p>
      <div className="instance-badges">
        <InstanceStatus instance={instance} />
        {['creator_studio', 'managed_project'].includes(instance.instanceType) && (
          <span>{t(instance.instanceType === 'creator_studio' ? 'ui.studio' : 'ui.managed')}</span>
        )}
        {collection && (
          <span className={`collection-tag accent-${collection.accent}`}>{collection.name}</span>
        )}
      </div>
    </Link>
  );
}
