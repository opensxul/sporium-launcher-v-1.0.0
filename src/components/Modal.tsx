import { useEffect, useId, useRef } from 'react';
import type { ReactNode } from 'react';
import { X } from 'lucide-react';
import { useFoundation } from '../app/context';

export function Modal({
  title,
  onClose,
  busy = false,
  className = '',
  children,
}: {
  title: string;
  onClose: () => void;
  busy?: boolean;
  className?: string;
  children: ReactNode;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const id = useId();
  const previous = useRef<HTMLElement | null>(null);
  const disabled = useRef(busy);
  useEffect(() => {
    disabled.current = busy;
  }, [busy]);
  const { t } = useFoundation();
  useEffect(() => {
    previous.current ??= document.activeElement as HTMLElement;
    const element = dialog.current;
    element?.showModal();
    return () => {
      element?.close();
      previous.current?.focus();
    };
  }, []);
  return (
    <dialog
      ref={dialog}
      className={`modal editor-modal ${className}`}
      aria-labelledby={id}
      onCancel={(event) => {
        event.preventDefault();
        event.stopPropagation();
        if (!disabled.current) onClose();
      }}
    >
      <header className="modal-header">
        <h2 id={id}>{title}</h2>
        <button
          type="button"
          className="icon-button"
          disabled={busy}
          onClick={onClose}
          aria-label={t('common.close')}
        >
          <X size={19} />
        </button>
      </header>
      <div className="modal-content">{children}</div>
    </dialog>
  );
}
