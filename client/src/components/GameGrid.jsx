import GameCard from './GameCard';
import { slugify } from '../slugify';

// Only the first rows get a staggered entrance — see the matching cap in App.css.
const STAGGER_LIMIT = 24;

export default function GameGrid({ games, metadata = {}, installed = {}, onLaunch, onInstall }) {
  if (games.length === 0) {
    return <p className="empty">No games match your filters yet.</p>;
  }

  return (
    <div className="game-grid">
      {games.map((game, index) => (
        <GameCard
          key={game.id}
          game={game}
          coverUrl={game.coverUrl ?? metadata[slugify(game.title)]?.coverUrl ?? null}
          index={index < STAGGER_LIMIT ? index : undefined}
          installed={Boolean(installed[game.id])}
          onLaunch={onLaunch}
          onInstall={onInstall}
        />
      ))}
    </div>
  );
}
