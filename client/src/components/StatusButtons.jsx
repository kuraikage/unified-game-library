import { STATUSES } from '../gameStatus';

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
