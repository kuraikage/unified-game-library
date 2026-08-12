/**
 * Compact activity line for the library view. Renders nothing when there's nothing to
 * report, so it never takes space during normal browsing.
 */
export default function StatusStrip({
  refreshing,
  job,
  missingCount,
  igdbConfigured,
  onEnrich,
  onDismissError,
}) {
  const hasError = Boolean(job.error);
  const pending = igdbConfigured && !job.running && missingCount > 0;

  if (!refreshing && !job.running && !pending && !hasError) return null;

  const percent = job.total > 0 ? Math.round((job.completed / job.total) * 100) : 0;

  return (
    <div className={`status-strip${hasError ? ' has-error' : ''}`}>
      {refreshing && (
        <span className="status-item">
          <span className="spinner" aria-hidden="true" />
          Refreshing your Steam library...
        </span>
      )}

      {job.running && (
        <span className="status-item grow">
          <span className="spinner" aria-hidden="true" />
          <span>
            Fetching genres &amp; art — <strong>{job.completed}</strong> of {job.total}
          </span>
          <span className="progress" role="progressbar" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
            <span className="progress-fill" style={{ width: `${percent}%` }} />
          </span>
          <span className="muted">{percent}%</span>
        </span>
      )}

      {pending && !refreshing && (
        <span className="status-item grow">
          <span>
            <strong>{missingCount}</strong> games have no genres, tags or artwork yet
          </span>
          <button type="button" className="link-button" onClick={onEnrich}>
            Fetch now
          </button>
        </span>
      )}

      {hasError && (
        <span className="status-item error">
          {job.error}
          <button type="button" className="link-button" onClick={onDismissError}>
            Dismiss
          </button>
        </span>
      )}
    </div>
  );
}
