import type { PackPreview } from '../bindings/core';
export function showPackPreview(preview: PackPreview) {
  window.dispatchEvent(new CustomEvent('sporium-pack-preview', { detail: preview }));
}
