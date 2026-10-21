<div align="center">

# GitTree

**A fast, free and open-source Git desktop client for macOS and Windows.**

Built with Rust and Tauri. Stays smooth on repositories with hundreds of thousands of commits and thousands of branches.

[![Build](https://github.com/ADroidCoder/GItTree/actions/workflows/build.yml/badge.svg)](https://github.com/ADroidCoder/GItTree/actions/workflows/build.yml)
[![Release](https://img.shields.io/github/v/release/ADroidCoder/GItTree?include_prereleases&sort=semver)](https://github.com/ADroidCoder/GItTree/releases/latest)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey)

English · [简体中文](README.zh-CN.md)

[**Download**](https://github.com/ADroidCoder/GItTree/releases/latest) · [Features](#features) · [Performance](#performance) · [Building from source](#building-from-source)

</div>

<p align="center"><img src="docs/screenshot.png" alt="GitTree screenshot" width="100%"></p>

---

## Why GitTree

Most Git GUIs slow down as a repository grows: opening takes seconds, scrolling stutters, the branch list freezes. GitTree is designed around the opposite goal — **every interaction must stay instant, no matter how big the repository is.**

- **Fast by design** — history is read from Git's commit-graph index, the graph layout is computed in O(n), and every list is virtualized. Opening the Git project itself (85k commits, 1,000+ refs) takes about 120 ms.
- **Native and lightweight** — an installer of only a few megabytes, built on the system WebView instead of a bundled Chromium.
- **Behaves exactly like the command line** — all write operations are delegated to your installed `git`, so hooks, GPG signing, SSH keys and credential managers work the same way they do in the terminal.
- **Free and open source** — Apache 2.0, no account, no telemetry, no paid tier.

## Download

Get the latest installer from the [**Releases page**](https://github.com/ADroidCoder/GItTree/releases/latest).

| Platform | File |
|---|---|
| macOS (Apple Silicon) | `GitTree_<version>_aarch64.dmg` |
| macOS (Intel) | `GitTree_<version>_x64.dmg` |
| Windows 10 / 11 (x64) | `GitTree_<version>_x64-setup.exe` or `GitTree_<version>_x64_en-US.msi` |

**Requirement:** [Git](https://git-scm.com/downloads) must be installed and available on `PATH`.

> [!NOTE]
> The installers are not code-signed yet.
> - **macOS:** if the system says the app "is damaged" or "cannot be opened", run `xattr -cr /Applications/GitTree.app` once, or right-click the app and choose **Open**.
> - **Windows:** if SmartScreen appears, click **More info → Run anyway**.

## Features

### History and branches
- **Commit graph** in the style of GitKraken, with a dark theme by default (light and system themes available).
- **Multiple repositories in tabs**; open tabs are restored on the next launch.
- **Focus mode** — show only the current branch, or any chosen set of branches, to declutter repositories with many branches. Filters are remembered per repository.
- **Search** commits by message, author or hash.
- **Drag and drop** a branch onto another branch to merge or rebase.

### Everyday operations
- Stage, unstage and discard by file, by hunk or **by individual lines**; commit and amend.
- **Syntax-highlighted diffs with word-level changes** — the exact characters that changed within a line are highlighted.
- **Compare any two branches, tags or commits** — GitHub-style “changes since the common ancestor”, or a direct comparison of both ends, with the list of commits in between.
- Fetch, pull and push with **live progress, elapsed time and cancellation**. Every executed git command and its full output is available in the log panel.
- **In-app authentication** — when Git asks for a username, password, personal access token or SSH key passphrase, GitTree shows a dialog. Credentials are stored by your system credential manager (macOS Keychain / Windows Credential Manager).
- **Background auto-fetch** (every 10 minutes by default, configurable or off).
- **Stash** — browse stashed files and diffs (including untracked files), stash only selected files, apply / pop / drop.
- Branches, tags, merge, rebase, cherry-pick, revert and reset from context menus. Rename branches (optionally on the remote too), push and delete tags on remotes, and force push with `--force-with-lease`.
- **Remotes** — add, rename, remove and change URLs; copy a repository URL (credentials are stripped) or open its web page in the browser.
- **Switch branches with uncommitted changes** — GitTree offers to stash them, switch, and restore them on the new branch.
- **Clone** with progress, or **create** a new repository.
- **Open in** your editor (VS Code by default; Cursor, IntelliJ IDEA etc. supported), terminal, or Finder / Explorer.

### Advanced tools
- **Undo** (⌘Z / Ctrl+Z) for commits, checkouts, branch and tag changes, merges, rebases, cherry-picks, resets, pulls and Git Flow actions — up to 50 steps. Undo is refused if the repository was changed elsewhere in the meantime, so newer work is never overwritten.
- **Conflict resolution** — side-by-side *current* vs *incoming* view for each conflict; pick a side, take both, use the base version or edit by hand. Non-UTF-8 files (e.g. GBK) are preserved byte for byte.
- **Interactive rebase** without a terminal editor — reorder commits by dragging, squash or drop them, and edit messages inline.
- **File history** that follows renames, and **Blame** with per-line author, date and commit.
- **Git Flow** — start and finish feature / bugfix / release / hotfix branches from one panel. Compatible with the `git flow` CLI and SourceTree configuration.

### App
- **Command palette** (⌘K / Ctrl+K) — run any command, switch branches, check out remote branches, jump to tags or open repositories by typing a few letters.
- **Reminders and notifications** — recurring reminders such as “drink water every hour” or “stand up every two hours” within your working hours, daily reminders for tasks that are due or overdue, and notifications when a long push / pull / clone finishes or new commits arrive on your branch. Optionally keeps running in the menu bar / system tray after the window is closed.
- **English and Chinese UI** — follows the system language; switch any time in Settings (⚙).
- **Automatic updates** — GitTree checks GitHub Releases in the background and installs new versions in one click. Updates are signed and verified before installing.

### Task board
A global Kanban board (to do / in progress / review / done, columns customizable) with assignees, priorities, due dates and checklists. Finished tasks are archived automatically after a configurable number of days and stay searchable. Tasks can be linked to a repository and branch. Data is stored locally and never written into your repositories.

## Keyboard shortcuts

On Windows, use Ctrl instead of ⌘.

| Shortcut | Action |
|---|---|
| ⌘K / ⌘⇧P | Command palette |
| ⌘O / ⌘T | Open a repository in a new tab |
| ⌘1 – ⌘9 | Switch to tab N |
| ⌘F | Search commits (Enter / Shift+Enter for next / previous) |
| ⌘R / F5 | Refresh |
| ⌘Z | Undo the last operation (undoes text when an input is focused) |
| ↑ ↓ / j k | Move between commits |
| ⌘Enter | Commit (in the commit message box) |
| Esc | Close the diff |

## Performance

Measured on the [git/git](https://github.com/git/git) repository (≈85,000 commits, 1,000+ refs):

| Scenario | Time |
|---|---|
| Open repository (with commit-graph) | ~120 ms |
| Open repository (without commit-graph) | ~520 ms |
| Graph layout | 3 ms |
| Load messages for one screen (60 rows) | 2 ms |
| Range query while scrolling | ~1 µs |

How it is achieved:

- **Commit-graph first** — history is read from Git's `commit-graph` file without inflating commit objects. If the file is missing, GitTree generates it in the background (the same file `git gc` writes).
- **Lazy loading** — authors and messages are loaded for visible rows first; the rest are filled in on background threads.
- **O(n) graph layout** with a chunked index (256 rows per chunk), so each scroll query costs about a microsecond.
- **Full virtualization** — only the visible rows of the commit list and diffs are rendered, branch lines are drawn on a canvas, and a custom scrollbar avoids the browser's maximum element height, so even millions of commits scroll smoothly.

Run the benchmark yourself:

```bash
cargo run --release -p gittree-core --example bench -- /path/to/repo
```

## Building from source

**Prerequisites**

- [Rust](https://rustup.rs/) 1.85 or newer
- [Node.js](https://nodejs.org/) 20 or newer
- Git
- Windows only: [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (bundled with Windows 10/11) and the Visual Studio C++ Build Tools

```bash
git clone https://github.com/ADroidCoder/GItTree.git
cd GItTree
npm install

npm run tauri dev                           # run in development mode
GITTREE_REPO=/path/to/repo npm run tauri dev # open a repository on startup
npm run tauri build                         # build installers
```

Build output:

- macOS: `target/release/bundle/dmg/*.dmg` and `target/release/bundle/macos/GitTree.app`
- Windows: `target/release/bundle/msi/*.msi` and `target/release/bundle/nsis/*.exe`

**Tests**

```bash
cargo test -p gittree-core   # core library
npm run check                # frontend type check
```

**Releases** are built by [GitHub Actions](.github/workflows/build.yml). Every push builds installers for all platforms (downloadable from the run's *Artifacts*); pushing a tag such as `v0.2.0` also publishes a GitHub Release with the installers attached.

## Project structure

```
crates/gittree-core/   Core library, UI-independent and testable on its own
  src/log.rs           Commit history, refs, ahead/behind counts
  src/graph.rs         Branch graph lane layout
  src/diff.rs          Commit details, file diffs, hunk patches
  src/ops.rs           All write operations, executed by the system git
  src/flow.rs          Git Flow
  src/undo.rs          Undo
  src/conflict.rs      Conflict parsing and resolution
  src/history.rs       File history and blame
  src/rebase.rs        Interactive rebase
src-tauri/             Tauri desktop app exposing the core library to the UI
src/                   Svelte 5 user interface
```

**Tech stack:** Rust · [gitoxide](https://github.com/GitoxideLabs/gitoxide) · [libgit2](https://libgit2.org/) · [Tauri 2](https://tauri.app/) · [Svelte 5](https://svelte.dev/) · TypeScript

## Contributing

Issues and pull requests are welcome. Before submitting a pull request, please make sure `cargo test -p gittree-core` and `npm run check` pass.

## License

GitTree is licensed under the [Apache License 2.0](LICENSE). You may use it for personal and commercial purposes, modify it and redistribute it, provided you keep the copyright and license notices and state any changes you made.
