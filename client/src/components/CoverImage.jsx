import { useEffect, useState } from 'react';

/**
 * Tries each candidate URL in turn, falling through on load failure.
 *
 * Steam's portrait art (`library_600x900`) doesn't exist for every app — older titles
 * only ever got the landscape header — so a fallback chain is needed rather than a
 * single URL.
 */
export default function CoverImage({ sources, alt = '', fallbackText }) {
  const candidates = sources.filter(Boolean);
  const [index, setIndex] = useState(0);

  // Reset when the candidate list changes, e.g. once IGDB art arrives for this game.
  useEffect(() => setIndex(0), [candidates.join('|')]);

  if (index >= candidates.length) {
    return <div className="cover-fallback">{fallbackText}</div>;
  }

  return (
    <img
      src={candidates[index]}
      alt={alt}
      loading="lazy"
      onError={() => setIndex((i) => i + 1)}
    />
  );
}
