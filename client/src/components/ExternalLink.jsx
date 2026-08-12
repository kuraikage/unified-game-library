import { api } from '../api';

/**
 * In a desktop webview a plain <a target="_blank"> either does nothing or navigates the app
 * window away. This hands the URL to Rust, which only opens https links in the real browser.
 */
export default function ExternalLink({ href, children, className }) {
  return (
    <a
      href={href}
      className={className}
      onClick={(e) => {
        e.preventDefault();
        api.openExternal(href).catch(() => {});
      }}
    >
      {children}
    </a>
  );
}
