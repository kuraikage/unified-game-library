import { DriveIcon, InstallIcon, PlayIcon } from '../icons';

function formatPlaytime(minutes) {
  if (minutes === null || minutes === undefined) return null;
  if (minutes === 0) return 'Not played';
  const hours = minutes / 60;
  return hours < 10 ? `${hours.toFixed(1)} hrs` : `${Math.round(hours)} hrs`;
}

export default function GameCard({ game, coverUrl, index, installed, onLaunch, onInstall }) {
  const playtime = formatPlaytime(game.playtimeMinutes);

  return (
    <div
      className={`game-card${installed ? ' is-installed' : ''}`}
      style={index === undefined ? undefined : { '--i': index }}
    >
      <div className="cover">
        {coverUrl ? (
          <img src={coverUrl} alt="" loading="lazy" onError={(e) => (e.target.style.display = 'none')} />
        ) : (
          <div className="cover-fallback">{game.title.slice(0, 1)}</div>
        )}
        {/* Provenance lives in one chip: "steam" or "steam · family". */}
        <span
          className={`badge badge-${game.platform}${game.shared ? ' badge-shared' : ''}`}
          title={game.shared ? 'Shared with you via Steam Family' : undefined}
        >
          {game.platform}
          {game.shared && <span className="badge-sep">· family</span>}
        </span>
        {installed && (
          <span className="badge badge-installed" title="Installed on this PC">
            <DriveIcon />
            installed
          </span>
        )}

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
