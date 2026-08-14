import GameCard from './GameCard';
import { slugify } from '../slugify';
import { coverSources } from '../covers';

// Only the first rows get a staggered entrance — see the matching cap in App.css.
const STAGGER_LIMIT = 24;

export default function GameGrid({
  games,
  metadata = {},
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
    <div className="game-grid">
      {games.map((game, index) => {
        const slug = slugify(game.title);
        return (
          <GameCard
            key={game.id}
            game={game}
            coverSources={coverSources(game, metadata[slug])}
            index={index < STAGGER_LIMIT ? index : undefined}
            installed={Boolean(installed[game.id])}
            status={statuses[slug]?.status ?? null}
            onLaunch={onLaunch}
            onInstall={onInstall}
            onStatusChange={onStatusChange}
            onOpen={onOpen}
          />
        );
      })}
    </div>
  );
}
