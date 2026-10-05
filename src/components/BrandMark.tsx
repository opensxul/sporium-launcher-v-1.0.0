export function BrandMark() {
  return (
    <span className="brand-mark" aria-hidden="true">
      <img
        src="/branding/mark-128.png"
        srcSet="/branding/mark-128.png 1x, /branding/mark-256.png 2x"
        width={48}
        height={48}
        alt=""
      />
    </span>
  );
}
