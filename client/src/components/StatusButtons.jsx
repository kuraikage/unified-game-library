import { STATUSES } from '../gameStatus';

/**
 * A mouse click leaves the button focused, which keeps the card's hover overlay open
 * via :focus-within even after the pointer leaves. Blurring only for real pointer
 * clicks (event.detail > 0) fixes that while leaving keyboard activation — where
 * detail is 0 — with focus intact.
 */
export function blurIfPointerClick(event) {
  if (event.detail > 0) event.currentTarget.blur();
}

/**
 * Three toggles for a game's play status. Clicking the active one clears it, so the
 * same control both sets and unsets without needing a separate "backlog" button.
 */
export default function StatusButtons({ value, onChange, size = 'md' }) {
  return (
    <span className={`status-buttons status-buttons-${size}`} role="group" aria-label="Play status">
      {STATUSES.map(({ value: status, label, Icon }) => {
        const active = value === status;
        return (
          <button
            key={status}
            type="button"
            className={`status-button${active ? ` active is-${status}` : ''}`}
            aria-pressed={active}
            title={active ? `${label} — click to clear` : `Mark as ${label.toLowerCase()}`}
            onClick={(e) => {
              e.stopPropagation();
              blurIfPointerClick(e);
              onChange(active ? null : status);
            }}
          >
            <Icon />
            <span className="sr-only">{label}</span>
          </button>
        );
      })}
    </span>
  );
}
