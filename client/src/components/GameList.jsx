import { slugify } from '../slugify';
import { DriveIcon, InstallIcon, PlayIcon } from '../icons';
import { statusMeta } from '../gameStatus';
import StatusButtons from './StatusButtons';

function formatPlaytime(minutes) {
  if (minutes === null || minutes === undefined) return '—';
  if (minutes === 0) return 'Not played';
  const hours = minutes / 60;
  return hours < 10 ? `${hours.toFixed(1)} hrs` : `${Math.round(hours)} hrs`;
}

function Pills({ items }) {
  if (!items || items.length === 0) return <span className="muted">—</span>;
  return (
    <div className="pills">
      {items.map((item) => (
        <span key={item} className="pill">
          {item}
        </span>
      ))}
    </div>
  );
}

export default function GameList({
  games,
  metadata,
  installed = {},
  statuses = {},
  onLaunch,
  onInstall,
  onStatusChange,
  onOpen,
}) {
  if (games.length === 0) {
    return <p className="empty">No games match your filters yet.</p>;
  }

  return (
    <div className="game-list">
      <table>
        <thead>
          <tr>
            <th></th>
            <th>Title</th>
            <th>Platform</th>
            <th>Playtime</th>
            <th>Genres</th>
            <th>Tags</th>
            <th>Status</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {games.map((game) => {
            const slug = slugify(game.title);
            const entry = metadata[slug];
            const cover = game.coverUrl ?? entry?.coverUrl ?? null;
            const isInstalled = Boolean(installed[game.id]);
            const status = statuses[slug]?.status ?? null;
            const meta = statusMeta(status);
            return (
              // Same behaviour as a card: the row opens the detail panel, and the
              // controls inside it stop their own clicks so they never also open it.
              <tr
                key={game.id}
                className="game-row"
                tabIndex={0}
                aria-label={`${game.title} — details`}
                onClick={() => onOpen(game)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    onOpen(game);
                  }
                }}
              >
                <td className="list-cover">
                  {cover ? (
                    <img src={cover} alt="" loading="lazy" onError={(e) => (e.target.style.display = 'none')} />
                  ) : (
                    <div className="cover-fallback small">{game.title.slice(0, 1)}</div>
                  )}
                </td>
                <td>
                  <span className="title-cell">
                    {game.title}
                    {isInstalled && (
                      <span className="badge badge-installed" title="Installed on this PC">
                        <DriveIcon />
                        installed
                      </span>
                    )}
                  </span>
                </td>
                <td>
                  <span className="platform-cell">
                    <span
                      className={`badge badge-${game.platform}${game.shared ? ' badge-shared' : ''}`}
                      title={game.shared ? 'Shared with you via Steam Family' : undefined}
                    >
                      {game.platform}
                      {game.shared && <span className="badge-sep">· family</span>}
                    </span>
                  </span>
                </td>
                <td>{formatPlaytime(game.playtimeMinutes)}</td>
                <td>
                  <Pills items={entry?.genres} />
                </td>
                <td>
                  <Pills items={entry?.tags} />
                </td>
                <td>
                  <span className="status-cell">
                    {meta && (
                      <span className={`badge badge-status is-${status}`}>
                        <meta.Icon />
                        {meta.short}
                      </span>
                    )}
                    <StatusButtons
                      value={status}
                      size="sm"
                      onChange={(next) => onStatusChange(game, next)}
                    />
                  </span>
                </td>
                <td>
                  <button
                    type="button"
                    className={`action-btn compact${isInstalled ? ' play' : ''}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      isInstalled ? onLaunch(game) : onInstall(game);
                    }}
                  >
                    {isInstalled ? <PlayIcon /> : <InstallIcon />}
                    <span>
                      {isInstalled ? 'Play' : game.platform === 'epic' ? 'Store' : 'Install'}
                    </span>
                  </button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
