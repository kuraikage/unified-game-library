import { useEffect, useRef } from 'react';
import { DriveIcon, InstallIcon, PlayIcon } from '../icons';
import { statusMeta } from '../gameStatus';
import StatusButtons from './StatusButtons';
import CoverImage from './CoverImage';

function formatPlaytime(minutes) {
  if (minutes === null || minutes === undefined) return null;
  if (minutes === 0) return 'Never played';
  const hours = minutes / 60;
  return hours < 10 ? `${hours.toFixed(1)} hours played` : `${Math.round(hours)} hours played`;
}

function formatReleased(seconds) {
  if (!seconds) return null;
  return new Date(seconds * 1000).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

// Steam's own wording ("Very Positive") plus the number behind it, which is what actually
// tells you whether the wording means anything.
function reviewSummary(entry) {
  if (entry?.reviewPercent === null || entry?.reviewPercent === undefined) return null;
  const count = entry.reviewCount ? ` of ${entry.reviewCount.toLocaleString()} reviews` : '';
  return `${entry.reviewPercent}% positive${count}`;
}

// Steam's wide key art, which exists for any game with an appid. The panel falls back to
// the portrait cover alone when there's no Steam page.
function heroUrl(game) {
  if (!game.id.startsWith('steam-')) return null;
  const appid = game.id.slice('steam-'.length);
  return `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/library_hero.jpg`;
}

export default function GameDetail({
  game,
  entry,
  coverSources,
  installed,
  status,
  onClose,
  onLaunch,
  onInstall,
  onStatusChange,
}) {
  const panelRef = useRef(null);
  const closeRef = useRef(null);

  // Escape closes, and focus is trapped while open so tabbing can't wander into the grid
  // behind the panel.
  useEffect(() => {
    // Remembered so closing returns you to the card you opened, rather than to the top
    // of the document.
    const returnTo = document.activeElement;
    closeRef.current?.focus();

    function onKeyDown(event) {
      if (event.key === 'Escape') {
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== 'Tab') return;

      const focusable = panelRef.current?.querySelectorAll(
        'button:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])'
      );
      if (!focusable?.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];

      // The status toggles blur themselves after a pointer click, which drops focus to
      // <body>. Tabbing from there would land in the grid behind the panel, so pull it
      // back in first.
      if (!panelRef.current.contains(document.activeElement)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
        return;
      }

      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }

    document.addEventListener('keydown', onKeyDown, true);
    return () => {
      document.removeEventListener('keydown', onKeyDown, true);
      if (returnTo instanceof HTMLElement && document.contains(returnTo)) returnTo.focus();
    };
  }, [onClose]);

  const meta = statusMeta(status);
  const playtime = formatPlaytime(game.playtimeMinutes);
  const released = formatReleased(entry?.releasedAt);
  const reviews = reviewSummary(entry);
  const hero = heroUrl(game);

  return (
    <div className="detail-backdrop" onClick={onClose}>
      <div
        className="detail-panel"
        role="dialog"
        aria-modal="true"
        aria-label={game.title}
        ref={panelRef}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="detail-hero">
          {hero && <img src={hero} alt="" className="detail-hero-art" loading="lazy" />}
          <div className="detail-hero-cover">
            <CoverImage sources={coverSources} fallbackText={game.title.slice(0, 1)} />
          </div>
          <button type="button" className="detail-close" onClick={onClose} ref={closeRef}>
            Close
          </button>
        </div>

        <div className="detail-body">
          <h2 className="detail-title">{game.title}</h2>

          <div className="detail-badges">
            <span className={`badge badge-${game.platform}${game.shared ? ' badge-shared' : ''}`}>
              {game.platform}
              {game.shared && <span className="badge-sep">· family</span>}
            </span>
            {installed && (
              <span className="badge badge-installed">
                <DriveIcon />
                installed
              </span>
            )}
            {meta && (
              <span className={`badge badge-status is-${status}`}>
                <meta.Icon />
                {meta.short}
              </span>
            )}
          </div>

          {entry?.shortDescription && <p className="detail-blurb">{entry.shortDescription}</p>}

          <dl className="detail-facts">
            {playtime && (
              <div>
                <dt>Playtime</dt>
                <dd>{playtime}</dd>
              </div>
            )}
            {reviews && (
              <div>
                <dt>Steam reviews</dt>
                <dd>{reviews}</dd>
              </div>
            )}
            {released && (
              <div>
                <dt>Released</dt>
                <dd>{released}</dd>
              </div>
            )}
            {entry?.developer && (
              <div>
                <dt>Developer</dt>
                <dd>{entry.developer}</dd>
              </div>
            )}
            {installed?.installDir && (
              <div className="detail-fact-wide">
                <dt>Installed at</dt>
                <dd className="detail-path">{installed.installDir}</dd>
              </div>
            )}
          </dl>

          {entry?.genres?.length > 0 && (
            <section className="detail-section">
              <h3>Genres</h3>
              <div className="pills">
                {entry.genres.map((g) => (
                  <span className="pill" key={g}>
                    {g}
                  </span>
                ))}
              </div>
            </section>
          )}

          {entry?.tags?.length > 0 && (
            <section className="detail-section">
              <h3>Tags</h3>
              <div className="pills">
                {entry.tags.map((t) => (
                  <span className="pill" key={t}>
                    {t}
                  </span>
                ))}
              </div>
            </section>
          )}

          {!entry?.genres?.length && !entry?.tags?.length && (
            <p className="hint">No genres or tags for this one yet.</p>
          )}

          <div className="detail-actions">
            {installed ? (
              <button type="button" className="action-btn play" onClick={() => onLaunch(game)}>
                <PlayIcon />
                <span>Play</span>
              </button>
            ) : (
              <button type="button" className="action-btn" onClick={() => onInstall(game)}>
                <InstallIcon />
                <span>{game.platform === 'epic' ? 'Store' : 'Install'}</span>
              </button>
            )}
            <StatusButtons value={status} onChange={(next) => onStatusChange(game, next)} />
          </div>
        </div>
      </div>
    </div>
  );
}
