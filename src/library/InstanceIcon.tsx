import { useEffect, useState } from 'react';
import type { Instance } from '../bindings/core';
import { backend } from '../services/backend';
import { ProjectIcon } from '../content/ProjectIcon';
const images = new Map<string, string>();
export function InstanceIcon({ instance }: { instance: Instance }) {
  const reference = instance.iconRef ?? '';
  const managed = /^(custom|builtin):/.test(reference);
  const [loaded, setLoaded] = useState<{ reference: string; image: string } | null>(null);
  useEffect(() => {
    if (!managed || !backend.isDesktop) return;
    let active = true;
    const cached = images.get(reference);
    void (cached ? Promise.resolve(cached) : backend.instanceIcon(instance.id))
      .then((image) => {
        if (
          image &&
          /^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(image) &&
          image.length < 700000
        ) {
          if (images.size > 100) images.clear();
          images.set(reference, image);
          if (active) setLoaded({ reference, image });
        }
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [managed, reference, instance.id]);
  const source = loaded?.reference === reference ? loaded.image : images.get(reference);
  return managed && source ? (
    <span className="project-icon">
      <img src={source} alt={instance.name} width={56} height={56} />
    </span>
  ) : (
    <ProjectIcon url={managed ? null : reference} title={instance.name} kind="modpack" />
  );
}
