import { useEffect, useLayoutEffect, useRef, useState } from 'react';

/**
 * Pill group where the selected option is a capsule that slides between items.
 * The indicator is positioned from real DOM measurements rather than fixed
 * widths, so options can be any length and it still lands exactly.
 */
export default function SegmentedControl({ options, value, onChange, size = 'md', ariaLabel }) {
  const containerRef = useRef(null);
  const [indicator, setIndicator] = useState({ left: 0, width: 0, ready: false });

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return undefined;

    const measure = () => {
      const active = container.querySelector('[data-active="true"]');
      if (!active) return;
      setIndicator({ left: active.offsetLeft, width: active.offsetWidth, ready: true });
    };

    measure();

    // Widths shift on resize and when webfonts settle, so keep the pill in sync.
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    for (const child of container.children) observer.observe(child);
    return () => observer.disconnect();
  }, [value, options]);

  useEffect(() => {
    if (!document.fonts?.ready) return;
    document.fonts.ready.then(() => {
      const active = containerRef.current?.querySelector('[data-active="true"]');
      if (active) setIndicator({ left: active.offsetLeft, width: active.offsetWidth, ready: true });
    });
  }, []);

  return (
    <div
      ref={containerRef}
      className={`segmented segmented-${size}`}
      role="tablist"
      aria-label={ariaLabel}
    >
      <span
        className="segmented-indicator"
        aria-hidden="true"
        style={{
          transform: `translateX(${indicator.left}px)`,
          width: `${indicator.width}px`,
          // Skip the slide on first paint so it doesn't fly in from the left.
          opacity: indicator.ready ? 1 : 0,
        }}
      />
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="tab"
          aria-selected={value === option.value}
          data-active={value === option.value}
          className="segmented-option"
          onClick={() => onChange(option.value)}
        >
          {option.icon}
          <span>{option.label}</span>
        </button>
      ))}
    </div>
  );
}
