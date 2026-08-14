import { memo } from 'react';
import { DriveIcon, InstallIcon, PlayIcon } from '../icons';
import { statusMeta } from '../gameStatus';
import StatusButtons, { blurIfPointerClick } from './StatusButtons';
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
  onOpen,
}) {
  const playtime = formatPlaytime(game.playtimeMinutes);
  const meta = statusMeta(status);

  return (
    // The card body opens the detail view. A button rather than a div so it is reachable
    // by keyboard and announced as interactive; the action buttons inside stop the click
    // from bubbling, so pressing Play never also opens the panel.
    <div
      className={`game-card${installed ? ' is-installed' : ''}${status ? ` is-${status}` : ''}`}
      style={index === undefined ? undefined : { '--i': index }}
      role="button"
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
      <div className="cover">
        <CoverImage sources={coverSources} fallbackText={game.title.slice(0, 1)} />

        {/* Persistent information, gathered into one strip along the bottom instead of
            scattered across corners. The scrim keeps it legible over any artwork. */}
        {/* Installed hangs as a pennant from the top edge, clear of the meta strip.
            The shadow lives on the wrapper because filter is applied before clip-path,
            so a filter on the clipped element itself would have its shadow clipped away. */}
        {installed && (
          <span className="cover-pennant" title="Installed on this PC">
            <span className="cover-pennant-shape">
              <DriveIcon />
            </span>
          </span>
        )}

        <div className="cover-meta">
          {/* Status reads as a word — the icon alone wasn't clear enough. */}
          {meta && (
            <span className="cover-meta-row">
              <span className={`badge badge-status is-${status}`} title={meta.label}>
                <meta.Icon />
                {meta.short}
              </span>
            </span>
          )}
          <span className="cover-meta-row">
            <span
              className={`badge badge-${game.platform}${game.shared ? ' badge-shared' : ''}`}
              title={game.shared ? 'Shared with you via Steam Family' : game.platform}
            >
              {game.platform}
              {game.shared && <span className="badge-sep">· family</span>}
            </span>
          </span>
        </div>

        {/* Actions only — these appear on hover, since they're things you do rather
            than things you need to see while scanning. The container is click-through
            (see App.css); each button stops its own click so pressing Play doesn't also
            open the detail panel. */}
        <div className="cover-actions">
          {installed ? (
            <button
              type="button"
              className="action-btn play"
              onClick={(e) => {
                e.stopPropagation();
                blurIfPointerClick(e);
                onLaunch(game);
              }}
            >
              <PlayIcon />
              <span>Play</span>
            </button>
          ) : (
            // Epic exposes no install action, so that button opens the launcher's store page.
            <button
              type="button"
              className="action-btn"
              onClick={(e) => {
                e.stopPropagation();
                blurIfPointerClick(e);
                onInstall(game);
              }}
            >
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
