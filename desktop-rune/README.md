# Threadkeeper desktop

A native macOS client built with [Rune](https://github.com/anomalyco/rune), with an embedded Rust backend using [PowerSync Native](https://github.com/powersync-ja/powersync-native). The desktop application does not need Node, a localhost server, or a PowerSync Service.

## Try it

Build/package/open from the repository root:

```sh
sh desktop-rune/scripts/launch.sh
```

Building needs Rust 1.99.1, Python 3 and Xcode command-line tools. Running the resulting `desktop-rune/dist/Threadkeeper.app` needs none of those build tools. Quit an older running copy before reopening the rebuilt app.

Opening the app loads saved entries and starts the Rust GitHub importer automatically. A Tokio runtime on background threads owns database and network work. Rune receives database change notifications directly; no HTTP server runs inside the app. Quitting stops the worker and releases its database lock. Only one desktop instance may own a particular database.

Without a token, startup shows a compact Threadkeeper welcome page with the app version. Get started opens the blocking GitHub setup screen; successful authentication expands the window to the workspace. Create a fine-grained personal access token with Public repositories selected, then paste it into the masked field. No extra permissions are needed for public data. For private repositories, select the repositories and grant Issues and Pull requests read-only access. The app validates REST and GraphQL access, saves the token in `~/.config/threadkeeper/credentials.json`, and opens the tracker immediately. Failed validation or storage keeps setup open. Quit app, Command-Q, or closing the window exits; Escape cannot dismiss first-time setup. If GitHub rejects a previously connected token, a reconnect screen opens automatically. Close or Escape returns to cached entries and local notes; a sidebar Reconnect GitHub button remains until a replacement token is validated. Replacement resumes sync without resetting data or restarting. Network errors and rate limits do not trigger reconnect.

Credential discovery checks an explicit `GITHUB_TOKEN`, then `~/.config/threadkeeper/credentials.json`. GitHub CLI and Keychain are not accessed. The credentials directory uses mode `0700`, and the file uses `0600`; writes replace the file atomically. This is plaintext storage protected by filesystem permissions, not encryption. Tokens are never logged or included in settings imports/exports. An existing token skips setup; Retry saved token rereads the file.

After upgrading from Keychain storage, paste your token once to save it in the new location. The app does not read or delete its old Keychain entry, avoiding migration prompts. That entry can be removed manually from Keychain Access if desired.

## Data and configuration

The desktop stores its own files in:

```text
~/Library/Application Support/Threadkeeper/
  tracker.sqlite
  tracker.config.json     (polling interval and maintainer-file location)
  repositories.json       (tracked repositories, managed in the UI)
  repositories.last-good.json (automatic recovery copy)
  maintainers.json
  customers.json
  people.local.json       (optional private affiliations)
  snapshot.json           (fast startup/preview cache)
  repository-scope.json   (persistent repository selection)
```

On first launch from this checkout, an empty desktop database imports all tracker tables from `data/tracker.sqlite` in a single transaction. The old database is opened read-only with a consistent SQLite read snapshot; its WAL is accounted for. Item IDs, engagement events, advice, repository relationships and poll checkpoints are preserved. A committed migration marker prevents subsequent imports from replacing desktop changes. The legacy source database remains untouched.

Defaults for repositories and maintainers are embedded in the executable. On first launch, configuration files from this checkout are copied when available, otherwise embedded defaults are written. Subsequent launches read the desktop's configuration files. Repository management is available from **Settings… → Repositories** in the sidebar, the repository popover, or **⌘,**. The preferences sheet supports GitHub URLs or owner/repo, tracking toggles, removal, and **Import file…**. Import opens the native file picker and replaces the draft list for review. It accepts both `repositories.json` and the legacy `tracker.config.json` format; tracking is unchanged until Save changes. Older files containing `group` still import; groups are ignored and omitted on save. Save changes applies without restarting; ⌘S saves the sheet while it is open. Cancel/Escape discard the draft.

`repositories.json` is authoritative after its first creation. It is seeded from the old tracker config once, preserving the existing tracked list. Its format is:

```json
{
  "repositories": [
    { "name": "powersync-ja/powersync-js", "enabled": true }
  ]
}
```

Edits on disk reload within about a second. Invalid edits keep the last working configuration and show an error; the recovery copy also works across app restarts. An open sheet never silently overwrites newer disk edits: use **Reload file** to load them before saving. Enabled additions/re-enables from the UI validate GitHub access using the same paced client; existing repos can still be edited/removed offline, and new repos may be added disabled until authentication is available.

Removing or disabling a repo hides its entries, removes it from the display picker, and stops further polling. Saved items, advice, engagement, and checkpoints remain. Adding/enabling it again resumes tracking. An already-running request may finish; subsequent import steps check whether the repo is still enabled. The multiselect dropdown remains a display preference and does not change this tracked-repository configuration.

Environment overrides when launching the executable directly:

- `TRACKER_DESKTOP_DATA_DIR`: alternate desktop directory, useful for isolated tests.
- `TRACKER_IMPORT_DB`: legacy database to import once into an empty desktop database.
- `TRACKER_CONFIG`: alternate repository configuration; its maintainer/people paths are resolved relative to that file.
- `TRACKER_PEOPLE_FILE`: alternate private affiliations file.
- `TRACKER_GITHUB_POLLING=0`: disable credential discovery and GitHub polling for offline checks.

The Rust core is reusable independently of Rune in `../tracker-core/`. PowerSync Native is pinned to revision `dae36642d698c9e7dd19917afd998dc1e5307c88`; Rune is pinned to `2bdfae16568782a56cb93818944d05b93546bce5`. PowerSync core and SQLite are statically linked into the executable.

## Controls and refresh

- The sidebar selects an engagement pen. The searchable repository popover above the list supports checkboxes, Select all / Deselect all, and an Only shortcut per repo. Counts and manual GitHub refresh use the same selection, alongside search and issue/PR type.
- Repository selection is saved across launches. Unchecking repos from **All repositories** keeps an “all except…” preference that includes newly added repos. **Only** and **Deselect all** start an explicit subset that does not include new repos automatically. Search within the popover does not change the main list search. Click outside, press Escape, or choose Done to close it.
- **Reset search & type** preserves the repository and pen. **View all follow-up** leaves an empty pen without changing repository scope.
- **Refresh / ⌘R** requests a GitHub poll for the selected repository or all repositories. It imports deltas and all eligible discussion snapshots in scope, shows progress, coalesces repeated clicks and honors retry/auth cooldowns. Success messages disappear after 15 seconds; errors remain visible.
- Automatic polling uses the persisted checkpoints and a minimum five-minute interval, plus bounded initial/history/reconciliation steps. Requests remain serialized and paced at least one second apart. Pagination failures never advance a checkpoint.
- The UI reloads after database changes and every 60 seconds to update age pens; these local reloads do not trigger hard GitHub polls.
- Open an item directly with the card’s ↗ button (hover/focus or selected card), **right-click → Open in GitHub**, or the clickable title in its details pane. These actions preserve the selected card.
- `⌘F` searches, `↑`/`↓` select entries, `⌘O` opens GitHub, and `⌘S` saves local notes. Escape clears search.
- The sun toggles appearance. The note icon shows or hides Notes. Notes remain private to this tracker, and cards show an indicator when saved notes are present. Labels and PR/draft badges appear on list entries.

## Verification and headless previews

Run from the repository root:

```sh
cargo +1.99.1 test --workspace
cargo +1.99.1 build -p threadkeeper

# Open and query the actual embedded backend with no UI and no GitHub traffic:
TRACKER_DESKTOP_DATA_DIR=.local/rust-check TRACKER_GITHUB_POLLING=0 \
  desktop-rune/target/debug/threadkeeper --check-backend

# Render cached UI state without starting the worker or writing advice:
desktop-rune/target/debug/threadkeeper --snap .local/screens/rune-dark.png --theme dark --scale 2

# Explicitly inspect an isolated import, always offline:
cargo +1.99.1 run -p tracker-core --bin tracker-inspect -- \
  --data-dir .local/import-check --import-db data/tracker.sqlite

# Optional live, read-only GitHub verification: at most one issue and one PR
# from the first open-item page, with full discussion pagination. Use a new directory:
cargo +1.99.1 run -p tracker-core --bin tracker-smoke -- \
  powersync-ja/powersync-js .local/new-rust-smoke

# Optional live startup check: verifies the automatic worker (new directory):
cargo +1.99.1 run -p tracker-core --bin tracker-worker-smoke -- \
  powersync-ja/powersync-dotnet .local/new-worker-smoke
```

Fixture tests cover storage/migration, external-author classification, engagement boundaries, advice, worker locks/shutdown and UI notifications, checkpoint recovery, HTTP pagination/ETags/rate limits, and Rune interactions. Snapshot previews use the desktop cache, falling back to an older checkout cache when available; they never start the backend. The packaged app uses the same binary tested above.

The Node web UI has been retired. This prototype still has session-only appearance/list preferences, lightweight Markdown presentation, and no in-app GitHub OAuth flow or automatic LLM calls.

### Reading discussions

Selecting an issue or PR loads its ordinary discussion comments below the description, in pages of 30. Use **Load more comments** for longer threads and **Refresh** in the discussion header to recheck GitHub. Submitted PR review summaries are included; code diffs and inline code threads remain on GitHub. Comment bodies are cached per page in the desktop data directory, revalidated when the entry changes or the cache is older than five minutes on a new request. Saved comments remain readable offline, with a warning when refresh fails. Requests use the same paced GitHub client and quota/backoff handling as polling. Each connection has its own **Open in GitHub** button; its title opens a detail preview when the entry is available locally. Previewing leaves card highlighting and filters unchanged, including across polling updates. **Back to original entry** returns to the selected card; clicking a follow-up card exits the preview. Notes and discussion stay attached to the entry being previewed.

### Settings and maintainers

**Settings… / ⌘,** opens a separate native window with Repositories, Maintainers and VIPs pages. Each page has its own draft and Save changes button; switching pages preserves unsaved edits. Cancel discards drafts when the window is reopened. Closing the native window preserves its draft until reopened.

Maintainers are stored in the `maintainersFile` configured in `tracker.config.json` (by default `maintainers.json` alongside it). The active path is shown in Settings. Add a GitHub username or profile URL, remove entries, import JSON, reload the file, and save. Import replaces the draft and only applies after Save changes. The file format is `{"maintainers":[{"githubId":"123","login":"alice"}]}`. Imports can omit `githubId`; saving resolves new users through the shared paced GitHub client and verifies supplied IDs. Existing entries can be removed while offline. Classification uses stable IDs, not username spelling.

Adding a maintainer excludes their open entries from follow-up; removing one makes their entries external again. Stored entries, comments, notes and engagement history remain. Valid direct edits to the roster file apply live. Invalid edits retain the last good roster, report an error, and recover from a `*.last-good.json` backup on restart. A save detects file changes made since the draft was opened and asks you to reload instead of overwriting them. Repository saves never replace the maintainer roster.

### VIPs

**Settings… → VIPs** manages `vips.json` in the desktop application-data directory. Add a GitHub username or profile URL, edit optional full name, company, and notes, or import a JSON roster. Import replaces the draft; **Save changes** validates new users with GitHub and applies it. Existing entries and metadata can be edited offline. Use the style dropdown to choose **Gold star** (default), **Neon** (bright green edge), **Outline** (gold border), **Subtle tint** (warm background), or **Badge only**. All styles also show a VIP badge on their issue and PR cards. The detail pane shows their metadata.

See [vips.example.json](../vips.example.json) for an import template. Supported fields are `login`, optional `githubId`, `fullName`, `company`, `notes`, and `highlight` (`gold-star`, `neon`, `outline`, `subtle-tint`, or `badge-only`). Saving fills in a stable `githubId`, so matching survives GitHub username changes. Direct edits to the active file require IDs and apply live; invalid edits keep the last good list and show an error. A last-good backup restores classification after a restart. Saving refuses to overwrite concurrent file edits.

VIP classification is independent of legacy people affiliations and maintainer membership. Maintainers remain excluded from the external follow-up view even if also listed as VIPs. Removing a VIP removes their highlight without deleting their issues, PRs or advice. This VIPs settings workflow belongs to the native desktop app.

### Repository include terms

Each repository has an optional **Include terms** field: comma-separated literal strings, matched case-insensitively against the title only of issues and PRs. Matching any term includes the entry; an empty field includes all entries. In JSON use `"includeTerms": ["powersync", "journeyapps"]`. Blank terms are removed and duplicate terms are combined. Descriptions, comments, labels and code diffs are not searched; this is not GitHub search syntax or a regular expression.

Filtering runs in Rust after fetching repository listings. Nonmatching entries remain cached but are hidden from follow-up counts and excluded from discussion polling. PR enrichment is skipped for never-matching PRs; previously included PRs continue receiving closure/merge updates. Editing terms immediately re-evaluates cached entries and schedules reconciliation for PR metadata. Newly eligible entries may show pending engagement until the next poll. Notes and stored records survive filter changes. The app still downloads listing pages for the entire repository, including history.

### Automatic sync status

The board shows automatic sync status beneath its repository/search controls. **Details** opens Repositories settings, where every repository has a status: queued for initial import, waiting for the next page in the shared queue, importing open entries/history (with page progress), checking for updates, syncing discussions, up to date (last checked and open matching count), or an error/retry notice. Disabled tracking and paused GitHub polling are shown explicitly. Status includes repositories with no matching cards and updates automatically as the importer works.

Include terms now match titles only. On the first launch after this update, cached entries are re-evaluated automatically; descriptions and comments do not qualify an entry. Saved notes remain stored.

Background snapshot updates compare entry content separately from polling status. Unchanged/status-only polls preserve the card list, selection, and note editor; identical detail responses are ignored. The sync indicator keeps a fixed height so changing progress messages do not move the list. Actual item changes and age-pen transitions still update the board. A periodic local detail check keeps connected previews current without resetting unchanged content.

### Developer work

A separate **Developer work** sidebar section follows a selected maintainer across **all enabled repositories** in Settings. Choose the GitHub profile from the list header or **Settings → Developer work**. The selection is saved immediately in `developer-profile.json`; `Chriztiaan` is the initial preference when present in the maintainer roster. Profiles are managed under Maintainers. The selected profile need not own the token: results use the token's available repository access, not another person's private notifications or unread state.

Queues are **Review requested**, **Assigned**, **Created**, and **Participating**. Participation includes authorship, mentions, and ordinary comments. Cards show each matching reason independently of customer badges. The same entry may appear in multiple queues, once per queue. Team-only review requests are excluded by checking the PR's current individual requested reviewers. Historical approvals do not suppress fresh requests. There is no local done/dismiss/snooze state.

Developer work ignores Follow-up's maintainer exclusions, title include terms, repository picker, and time pens. Follow-up's search/type/order preferences are restored when returning to it. Notes and entry identities are shared through the same local database. Disabled/removed repositories disappear from Developer work too.

Review and Assigned default to oldest outstanding first, using the latest matching request/assignment event from the issue timeline (falling back to creation time if GitHub provides no matching event). Created and Participating default to newest update first. **Oldest / Newest** reverses each list. **Include closed/merged** includes history for authored, assigned, and participating items; actionable sidebar counts remain open-only. Review requested always represents current open requests.

Developer work reads authors and assignees directly from the shared repository import, without search requests. It checks the local view each minute while active. Requested reviewers are saved alongside normal PR enrichment and reused across profiles; only missing or changed PR versions need an additional REST detail request. Unchanged PR enrichment is reused too. Exact assignment/review-request timestamps use targeted, version-cached timeline requests; each successful timeline is retained even if a later item fails. Created and Assigned membership stays available during network cooldowns. Open queues become complete after the initial open import, while closed/history coverage waits for backfill to finish. Reviewer coverage also waits for missing PR details.

Participating combines local authored items, imported discussion authors, and cached discussion pages with two discovery searches (mentions and commenter) per repository. These searches are paginated and cached for fifteen minutes in `developer-work/`; switching queues reuses them. Search pacing is at least 3.1 seconds per search. Failures retain saved data with a five-minute minimum retry cooldown. GitHub retry/reset deadlines are honored globally and persisted across restarts; navigation and manual refresh cannot bypass them. The UI shows one pause-until message and shows known partial counts with +, and … when no items have loaded. Search results exceeding GitHub's 1,000-result cap or reporting incomplete results remain incomplete rather than silently truncating membership. Cached entries remain readable on failure.

Discussion pages include ordinary comments plus submitted PR review summaries, with load-more pagination for both. Review and comment IDs are kept distinct. Contributions by the selected developer have a named badge and subtle tint; their authored description has an author badge. Inline code threads remain on GitHub, explicitly noted in the pane. This is not a personal notification inbox, and participation does not imply a response is owed.

Preview the new UI without network or credential access:

```sh
desktop-rune/target/debug/threadkeeper --snap .local/screens/developer-work.png --scene developer-work --theme dark --scale 1 --size 1380x880
desktop-rune/target/debug/threadkeeper --snap .local/screens/developer-settings.png --scene developer-settings --theme light --scale 1 --size 1080x800
```

Legacy `customers.json` files and `customers` arrays still load. `company` and legacy `organization` populate the separate Company field. Previously saved notes remain unchanged; text is never guessed or moved out of Notes. Existing installs keep their active file path. New files use `vips.json` with a `vips` array. Notes appear in Settings and the item details, never on cards.

Discussion bodies longer than five wrapped lines start with an ellipsis and **See more**. **See less** restores that preview. The separate **Collapse / Expand comment** control hides or restores the entire body, preserving the author header and the prior preview/expanded choice. Choices are session-local and scoped to the item and comment/review ID.

Author avatars and usernames on cards, item details, and discussion comments open their GitHub profile. Right-click for **Open GitHub profile** and **Add to VIPs… / Edit VIP…**. VIP actions open Settings with Notes focused and preserve existing drafts; Save changes is still required. Unknown authors and bot logins without a normal GitHub user profile are not exposed as profile links.

Follow-up ranges are exclusive buckets: under 24 hours, 24 hours–7 days, 7 days–1 calendar month, 1–6 calendar months, and older than 6 months (Cold & stale). Exactly six months remains in Last 6 months. Classification uses the latest engagement and is computed locally without a new GitHub sync.

### Offline demo and repeatable recordings

Enter `mock_token` on the Connect GitHub screen to load a fictional workspace. This sentinel is never sent to GitHub or saved as a credential. Demo mode disables polling and keeps edits in memory. Every date range and developer queue has example entries; relative ages use a fixed November 17, 2026 clock. Restarting resets demo data.

To start at onboarding even when you already have saved credentials:

```sh
THREADKEEPER_DEMO=1 desktop-rune/target/debug/threadkeeper
```

This launch skips saved credentials, real cached data, and backend startup. Only `mock_token` can connect in this mode. Alternatively, `GITHUB_TOKEN=mock_token` opens the demo workspace directly. Launch one app instance at a time.

Generate the tour on macOS:

```sh
cargo build -p threadkeeper --offline
python3 desktop-rune/scripts/demo-tour.py
```

The script replays keyed UI actions using Rune's virtual clock at a fixed window size and dark theme. It captures welcome/token setup, all date ranges, developer queues, scrolling and expanding comments, and Settings. The only published recording is `threadkeeper-demo.gif` at the repository root, embedded in the main README. Numbered screenshots, `actions.json`, the hash manifest, and optional MP4 go to the ignored `.local/demo-tour/` directory.

`--verify` replays twice and checks that every checkpoint screenshot is byte-identical. `--gif PATH` changes the GIF destination; `--output PATH` changes the diagnostics directory; `--fps N` changes the recording frame rate (default 12). Temporary frame images are removed after encoding. Swift/ImageIO encodes GIF locally; if `ffmpeg` is available, MP4 is also exported into the diagnostics directory. No GitHub access is required. Settings are shown within the captured app surface so the tour does not depend on macOS window placement.
