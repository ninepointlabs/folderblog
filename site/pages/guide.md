---
title: The guide
description: Install folderblog, connect GitHub once, and start publishing.
weight: 1
---

This is the short version. The [full step-by-step guide](https://github.com/ninepointlabs/folderblog/blob/main/docs/GUIDE.md) explains every feature and every command in plain words (it's also built in: `folderblog guide` or `man folderblog`). The [README](https://github.com/ninepointlabs/folderblog#readme) has the technical details, and every blog's own `AGENTS.md` documents theming completely.

## 1. Install

Packages for Arch, Debian/Ubuntu and Fedora, plus a static binary for any x86_64 Linux, are on the [releases page](https://github.com/ninepointlabs/folderblog/releases/latest). The [home page](/#install) has the one-line commands. With [autonomix-cli](https://github.com/ninepointlabs/autonomix-cli): `autonomix-cli add https://github.com/ninepointlabs/folderblog`.

**Updating:** install the new version the same way, then restart the service:

```sh
systemctl --user restart folderblog
```

On its first start with a new version it brings every blog up to date by itself: it adds the sample files the blog doesn't have yet (like `templates/`) and refreshes `AGENTS.md`. It never turns a feature on and never changes your theme, `blog.toml` or posts.

## 2. Prepare GitHub (once, about 5 minutes)

folderblog publishes through a small **GitHub App registered on your own account**, so the login belongs to you and nothing sits in between.

1. Open [github.com/settings/apps/new](https://github.com/settings/apps/new) and fill in:
   - **Name:** anything unique, like `folderblog-yourname`
   - **Homepage URL:** anything, like your GitHub profile
   - **Expire user authorization tokens:** keep ticked
   - **Enable Device Flow:** tick it
   - **Webhook → Active:** untick it
   - **Repository permissions:** Administration, Contents and Pages set to *Read and write*
   - **Where can it be installed:** *Only on this account*
2. Create it and copy the **Client ID** (it starts with `Iv23li`). You don't need a secret or a private key.
3. Click **Install App** and install it on your account for **All repositories**, so it can publish to the repositories folderblog creates.
4. Log in:

```sh
folderblog login github --client-id Iv23li...
```

Enter the code it shows at [github.com/login/device](https://github.com/login/device). The login renews itself and lasts about six months. You'll get a notification a week before it needs renewing, or straight away if it's revoked.

## 3. Start the service and create a blog

```sh
folderblog install-service
folderblog new myblog
echo "# Hello

My first post." > ~/Blogs/myblog/posts/hello.md
```

A minute later it's live at `https://yourname.github.io/myblog/`. `folderblog status` shows every blog, its last build and publish, and GitHub's Pages build.

No systemd user session (a container, a server, an SSH login)? `install-service` then fails with `systemctl --user daemon-reload failed`. Run `folderblog watch` in a terminal or `tmux` instead, or `loginctl enable-linger $USER`, log in again and retry.

Publishing somewhere other than GitHub Pages? Set `base_url` in `blog.toml` to the site's real address first: a blog made with `--no-github` starts with a placeholder like `https://myblog.example.com`. The [full guide](https://github.com/ninepointlabs/folderblog/blob/main/docs/GUIDE.md) has the `[deploy]` choices.

For your own domain: `folderblog new example.com --domain example.com` prints the DNS records to add.

## 4. Write

- `posts/` holds posts: a Markdown file, or a folder with `index.md` and its images.
- `pages/about.md` becomes `/about/`.
- `drafts/` shows only in `folderblog preview`.
- `static/` is copied as-is.
- Delete a file to unpublish it.

Front matter is optional: `title`, `date`, `tags`, `summary`, `image`, `layout`, `draft` and anything else you like, which themes can read. To fill it in by hand, copy `templates/post.md` from your blog folder into `posts/`. It has every field folderblog knows, each with a short note, and starts as a draft: delete the `draft: true` line when it's ready.

### A photo gallery

```sh
folderblog gallery on
```

Drop pictures into `gallery/` and they appear at `/gallery/`. A folder inside it is an album. To add a title, caption and tags, put a Markdown file with the same name beside the photo (`sunset.md` next to `sunset.jpg`):

```markdown
---
tags: [sea, evenings]
---
# Sunset over the bay

The whole sky went orange for about ten minutes.
```

Only resized copies are published, never your originals, so the GPS location in your phone's photos stays private.

### A podcast

```sh
folderblog podcast on        # adds podcast/ and the show and episode templates
folderblog podcast setup     # connects Cloudflare R2 for the audio, step by step
```

Put the show's title and description in `podcast/index.md` and its square cover art (1400 to 3000 px) beside it as `cover.jpg`. Then give each episode its own folder:

```
podcast/
  2026-10-30-first-episode/
    index.md          the show notes, with the episode's details at the top
    episode.mp3       MP3 for audio, MP4 for video
    transcript.vtt    optional
    chapters.json     optional
```

The easiest way to start an episode is to copy `templates/podcast-episode/` from your blog folder: its `index.md` lists every detail with a note. Run `folderblog check`. It gives you the episode's permanent `guid` line to paste in, and tells you about anything else to fix. `folderblog preview` plays the episode on your own computer.

To publish, remove `draft: true`, or run `folderblog podcast publish --episode first-episode`. folderblog uploads the audio, makes sure it's online, then publishes the episode page and the feed at `/podcast/feed.xml`. Submit that feed address to Apple Podcasts, Spotify and the others once, and new episodes reach them by themselves. Episodes stay on `/podcast/` unless you set `in_main_feed`.

**Let an agent do it.** `folderblog mcp` is an MCP server. Add it to Claude Code with `claude mcp add folderblog -- folderblog mcp` (or to Cursor or Claude Desktop), hand it an audio file and say "put this up". It makes a draft episode, writes the notes and chapters, checks everything, and publishes only after you say yes.

### Visitor counts (optional)

Off by default. To count page views with [GoatCounter](https://www.goatcounter.com), which is cookie-free and keeps no personal data, add your GoatCounter site (self-hosted or hosted) to `blog.toml`:

```toml
[analytics]
goatcounter = "https://stats.example.com"
```

Published builds then add GoatCounter's script to every page, with any theme. Pages are counted as `host/path`, so one GoatCounter site can serve several blogs (`host_prefix = false` turns that off). A visit to a missing page is counted as an event named `404 host/path` instead of a page view, so broken links stand out. `folderblog preview` never counts, and files in `static/` are copied untouched. Needs a folderblog newer than 0.2.2.

## 5. Make it yours

A theme is the `theme/` folder in the blog. Files ending in `.html` or `.jinja` are templates rendered to the same path, and everything else is copied. A route header at the top of a template sets its URL instead (the default `theme/archive.html` starts with `url = "/archive/"`, so it becomes `/archive/`, not `/archive.html`), and can make one template produce many pages:

```jinja
{#---
each = "tags"
url = "/tags/{{ item.slug }}/"
---#}
```

Hand the blog folder to a coding agent with a one-line design brief. It will find everything it needs in `AGENTS.md`, and can check its own work with:

```sh
folderblog preview                    # live reload, drafts included
folderblog preview --fixtures stress  # long titles, code, tables, empty pages
folderblog check                      # template errors (file:line), feed, broken links
folderblog data                      # every URL and what made it
folderblog data /archive/             # the exact template context for one of them
```
