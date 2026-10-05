import type { ComponentType, ReactNode } from 'react';
import type { LucideProps } from 'lucide-react';
import { AlertCircle, Search, X } from 'lucide-react';
import { useFoundation } from '../app/context';
import type { BackendError } from '../services/backend';

export function PageHeading({
  eyebrow,
  title,
  description,
  action,
}: {
  eyebrow?: string;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <header className="page-heading">
      <div>
        {eyebrow && <p className="eyebrow">{eyebrow}</p>}
        <h1>{title}</h1>
        <p className="page-description">{description}</p>
      </div>
      {action}
    </header>
  );
}

export function EmptyState({
  icon: Icon,
  title,
  body,
  children,
}: {
  icon: ComponentType<LucideProps>;
  title: string;
  body: string;
  children?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <div className="empty-icon">
        <Icon size={34} strokeWidth={1.35} />
      </div>
      <h2>{title}</h2>
      <p>{body}</p>
      {children}
    </div>
  );
}

export function SearchBar({
  value,
  onChange,
  label,
  clearLabel,
}: {
  value: string;
  onChange: (value: string) => void;
  label: string;
  clearLabel: string;
}) {
  return (
    <div className="search-bar">
      <Search size={18} aria-hidden="true" />
      <input
        type="search"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={label}
        aria-label={label}
      />
      {value && (
        <button
          type="button"
          className="icon-button"
          onClick={() => onChange('')}
          aria-label={clearLabel}
        >
          <X size={16} />
        </button>
      )}
    </div>
  );
}

export function ErrorNotice({ error, action }: { error: BackendError; action?: ReactNode }) {
  const { t } = useFoundation();
  return (
    <div className="error-notice" role="alert">
      <AlertCircle size={20} />
      <div>
        <p>{t(`error.${error.code}`)}</p>
        {action}
      </div>
    </div>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  description,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  description: string;
  disabled?: boolean;
}) {
  return (
    <label className="setting-row">
      <span>
        <span className="setting-label">{label}</span>
        <span className="setting-hint">{description}</span>
      </span>
      <span className="switch">
        <input
          type="checkbox"
          role="switch"
          checked={checked}
          onChange={(event) => onChange(event.target.checked)}
          disabled={disabled}
        />
        <span className="switch-track" aria-hidden="true" />
      </span>
    </label>
  );
}
