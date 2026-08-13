import { memo } from 'react';
import { DriveIcon, InstallIcon, PlayIcon } from '../icons';
import { statusMeta } from '../gameStatus';
import StatusButtons from './StatusButtons';
import CoverImage from './CoverImage';

function formatPlaytime(minutes) {
  if (minutes === null || minutes === undefined) return null;
  if (minutes === 0) return 'Not played';
  const hours = minutes / 60;
  return hours < 10 ? `${hours.toFixed(1)} hrs` : `${Math.round(hours)} hrs`;
}

function GameCard({
  game,
  coverSources,
  index,
  installed,
  status,
  onLaunch,
  onInstall,
  onStatusChange,
}) {
  const playtime = formatPlaytime(game.playtimeMinutes);
  const meta = statusMeta(status);

  return (
    <div
      className={`game-card${installed ? ' is-installed' : ''}${status ? ` is-${status}` : ''}`}
      style={index === undefined ? undefined : { '--i': index }}
    >
      <div className="cover">
        <CoverImage sources={coverSources} fallbackText={game.title.slice(0, 1)} />

        {/* Persistent information, gathered into one strip along the bottom instead of
            scattered across corners. The scrim keeps it legible over any artwork. */}
        <div className="cover-meta">
          <span
            className={`badge badge-${game.platform}${game.shared ? ' badge-shared' : ''}`}
            title={game.shared ? 'Shared with you via Steam Family' : game.platform}
          >
            {game.platform}
            {game.shared && <span className="badge-sep">· family</span>}
          </span>
          <span className="cover-meta-icons">
            {installed && (
              <span className="badge badge-icon badge-installed" title="Installed on this PC">
                <DriveIcon />
              </span>
            )}
            {meta && (
              <span className={`badge badge-icon badge-status is-${status}`} title={meta.label}>
                <meta.Icon />
              </span>
            )}
          </span>
        </div>

        {/* Actions only — these appear on hover, since they're things you do rather
            than things you need to see while scanning. */}
        <div className="cover-actions">
          {installed ? (
            <button type="button" className="action-btn play" onClick={() => onLaunch(game)}>
              <PlayIcon />
              <span>Play</span>
            </button>
          ) : (
            // Epic exposes no install action, so that button opens the launcher's store page.
            <button type="button" className="action-btn" onClick={() => onInstall(game)}>
              <InstallIcon />
              <span>{game.platform === 'epic' ? 'Store' : 'Install'}</span>
            </button>
          )}
          <StatusButtons value={status} onChange={(next) => onStatusChange(game, next)} />
        </div>
      </div>

      <div className="game-info">
        <p className="title" title={game.title}>
          {game.title}
        </p>
        {playtime && <p className="playtime">{playtime}</p>}
      </div>
    </div>
  );
}

// Filtering re-renders the whole grid; without this every surviving card re-renders
// even though nothing about it changed. Callbacks are stable via useCallback in App.
export default memo(GameCard);
