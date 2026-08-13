<div align="center">

<img src="client/public/icon.png" alt="UGLy" width="120" />

# UGLy

### **U**nified **G**ame **L**ibrar**y**

**All your games. One place.**

</div>

---

## What is this?

Your games are scattered across Steam and Epic, in launchers that don't talk to each other. So
"what should I play tonight?" turns into opening both, scrolling past hundreds of titles you forgot
you owned, and giving up.

UGLy — short for **Unified Game Library** — puts your entire library in one place and lets you
search through it to find the next game you actually want to play. Everything is enriched with
genres, tags and cover art, so you can search by mood instead of memory: `roguelike`, `co-op`,
`point and click`. Games already installed are marked and launch straight from the grid.

Still can't decide? Export the whole thing to CSV and throw it at an AI to pick something based on
your mood.

## Screenshots

<img src="docs/screenshots/library-grid.png" alt="Grid view showing the combined library with cover art and an installed badge" />

<sub>Grid view — Steam, Epic and Steam Family in one place, with installed games marked.</sub>

<img src="docs/screenshots/library-list.png" alt="List view showing title, platform, playtime, genres and tags with Play and Store actions" />

<sub>List view — genres and tags from IGDB, with Play or Store depending on whether it's installed.</sub>

<img src="docs/screenshots/settings.png" alt="Settings screen showing library counts, connection status and the import bookmarklets" />

<sub>Settings — library totals, connection status, and the one-time bookmarklet setup.</sub>

## Features

- **One library** — Steam and Epic Games side by side, including Steam Family shared games
- **Search that understands you** — matches titles, genres and tags, forgives punctuation
  (`point-and-click` = `point and click` = `pointandclick`) and knows common shorthand
  (`sci-fi` → *Science fiction*, `fps` → *Shooter*, `deckbuilder` → *Card & Board Game*)
- **Installed detection** — sees what's on disk across every Steam library folder and Epic's
  manifests, with Play / Install buttons that hand off to the right launcher
- **Genres, tags and artwork** from IGDB, fetched automatically for new games
- **CSV export** of whatever the current filter is showing — feed it to an LLM and let it pick
- **Grid and list views**, filterable by platform or installed state

## Install

Grab the latest installer from the [Releases](../../releases) page and run it. No Node, no Rust,
no terminal — it installs like any other Windows app.

> [!NOTE]
> The installer isn't code-signed, so Windows SmartScreen will show
> **"Windows protected your PC"**. Click **More info** → **Run anyway** to continue. Code signing
> certificates cost money and this is a free hobby project; the source is all here if you'd rather
> build it yourself.

## Setup

Everything below is optional until you need it — the app opens on the Settings tab if nothing is
configured yet.

### Steam

1. Get a Web API key from [steamcommunity.com/dev/apikey](https://steamcommunity.com/dev/apikey).
2. Find your SteamID64 at [steamid.io](https://steamid.io).
3. Paste both into **Settings** and save.

Your profile's game details need to be public for the API to return your library.

### IGDB (genres, tags, artwork) — optional

1. Register a free application at the
   [Twitch developer console](https://dev.twitch.tv/console/apps).
2. Paste the Client ID and Client Secret into **Settings**.

Lookups then happen on their own: whenever the library gains games with no metadata, UGLy fetches
just those in the background and shows live progress. Results are cached permanently, so only
genuinely new titles ever cost a request.

### Epic Games

Epic publishes no API for your library, and the reverse-engineered launcher OAuth flow other tools
use carries a real risk of account action — so UGLy doesn't touch it. Instead it reads your own
purchase history from your own logged-in browser session:

1. In **Settings**, click **Copy bookmarklet** under *Connect Epic*.
2. Create a browser bookmark and paste the copied text as its **URL** (one-time setup).
3. Visit [epicgames.com/account/transactions](https://www.epicgames.com/account/transactions),
   logged in.
4. Click the bookmark. Your library lands in UGLy automatically.

Refunded orders, games gifted to other people, and Unreal Engine Marketplace/Fab assets are all
filtered out.

### Steam Family shared games — optional

Steam's official API only returns games your own account owns, so family-shared titles need a
second bookmarklet that reads a short-lived session token. Same flow: copy it from **Settings**,
save it as a bookmark, then click it while logged in at
[store.steampowered.com](https://store.steampowered.com). The token is used once and never stored.

## Build from source

Requires [Node.js](https://nodejs.org) (build-time only) and the [Rust toolchain](https://rustup.rs).
On Windows you also need the MSVC build tools, which ship with Visual Studio.

```bash
npm install
npm run install:all
npm run tauri:dev
```

To produce an installer of your own:

```bash
npm run tauri:build
```

Output lands in `src-tauri/target/release/bundle/`.

To work on the UI in a normal browser with sample data (no Tauri, no real library):

```bash
VITE_UI_FIXTURE=1 npm run dev --prefix client
```

## Your data stays yours

- **Credentials** live in the **Windows Credential Manager**, encrypted per user. They're read
  only inside the Rust backend and are *never* sent to the app's UI — the interface only learns
  whether a key is set, never its value.
- **Your library** lives in a local SQLite database at `%APPDATA%\com.ugly.library\ugly.db`.
- **Nothing is uploaded anywhere.** The only outbound requests are to Steam, Epic and IGDB, made
  directly from your machine.
- The Epic and Steam Family imports run in *your* browser using *your* existing session. No
  passwords are entered into UGLy, and no long-lived tokens are stored.

## How it works

| Layer | Tech |
|---|---|
| UI | React + Vite, rendered in the system webview |
| Backend | Rust, exposed to the UI as Tauri commands |
| Storage | SQLite (`rusqlite`) |
| Credentials | Windows Credential Manager |
| Imports | A small local HTTP listener on port `43117` that the bookmarklets post to |

The shipped app contains no JavaScript runtime — Node is only used to build the frontend, which is
why the installer is a few megabytes rather than a few hundred.

## Known limitations

- **Windows only** for now. The launcher integration and credential storage are Windows-specific.
- **Imports are snapshots, not live syncs.** Re-run a bookmarklet when you want to refresh.
- **Downloads can't be paused or cancelled from UGLy** — it hands off to Steam or Epic, which own
  the download entirely.
- **Epic install** opens the game's store page in the Epic launcher, because Epic exposes no
  install action for a game you don't already own locally.
- **IGDB matching is by title**, so demos, playtests and bundled tools often won't resolve to a
  record and simply show no genres or tags.

## Contributing

Issues and pull requests are welcome. This started as a personal itch, so expect rough edges.

## License

[MIT](LICENSE) — do whatever you like with it.
