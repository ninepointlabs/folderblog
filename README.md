# folderblog

A folder is a blog. Save a Markdown file into it and the site rebuilds and deploys
seconds later. No front matter, build step, commit or deploy command is required.

One binary runs as a systemd user service. It watches a root folder (default `~/Blogs`).
Every subfolder with a `blog.toml` is an independent blog, each with its own theme, config
and deploy target.

```
~/Blogs/example.com/
  blog.toml   title, base_url, permalink, [deploy], [build] hooks, [params]
  posts/      *.md, or <slug>/index.md with images beside it
  pages/      about.md -> /about/; .html files render as templates
  drafts/     preview only
  static/     copied to the site root
  data/       JSON/TOML/YAML exposed to templates as `data`
  theme/      owns every byte of output except /feed.xml and /sitemap.xml
  AGENTS.md   the theme contract, generated for coding agents
  .blog/      engine state: first-seen dates, status, last good build (public/)
```

## Install

```sh
cargo install --path .
folderblog login github             # once; enter the code shown at github.com/login/device
folderblog new example.com          # creates ~/Blogs/example.com, published to GitHub Pages
folderblog install-service          # writes and starts ~/.config/systemd/user/folderblog.service
folderblog status
journalctl --user -u folderblog -f
```

## Commands

| command | |
|---|---|
| `new NAME` | new blog with the default theme and AGENTS.md |
| `build [BLOG]` | build into `.blog/public` (or `--out`) |
| `preview [BLOG]` | local server, live reload, drafts; `--fixtures stress\|empty\|one` |
| `check [BLOG]` | template errors (file:line), feed validation, broken links, collisions |
| `data [URL]` | exact template context for a URL as JSON |
| `watch` | the daemon |
| `deploy [BLOG]` | build and deploy now |
| `status` | per-blog build/deploy state; exit code 1 if anything is failing |
| `install-service` | systemd user unit (`--print`, `--no-start`) |
| `agents-md [BLOG]` | regenerate the contract section of AGENTS.md |

## Content rules

- Title: front matter `title`, else a leading heading (removed from the body), else the filename.
- Slug: filename minus any `YYYY-MM-DD-` prefix (bundle: folder name).
- Date: front matter `date`, else filename prefix, else the moment folderblog first saw
  the post, stored in `.blog/state.json` by slug so it never moves. File mtimes are never used.
- Content files are never written. Deleting a file unpublishes it.
- A malformed file (bad front matter, unparseable date, bad data file) fails the build;
  the live site keeps the last good build.

## Deploy targets

With `folderblog login github` done, `folderblog new NAME` writes a `github` target and
the first deploy creates the repo and turns on Pages; there is nothing to do on GitHub.
The login renews itself; if it expires or is revoked you get a desktop notification
(then a daily reminder), `status` says so, and pending posts publish within 5 minutes
of logging in again.

```toml
[deploy]
type = "github"                 # repo + Pages created automatically
owner = "you"
repo = "example.com"
# domain = "example.com"        # custom domain: CNAME + Pages setting (you add DNS)

[deploy]
type = "git"                    # pushes the output to a branch (GitHub Pages)
remote = "git@github.com:you/site.git"
branch = "gh-pages"

[deploy]
type = "command"                # anything else: rsync, rclone, wrangler...
command = 'rsync -a --delete "$FOLDERBLOG_OUT/" host:/var/www/site/'
```

To add a target, implement `deploy::Target` and add a `DeployConfig` variant.

## Development

```sh
cargo test            # content rules, routing, dates, feed, contract-vs-AGENTS.md
tests/e2e.sh          # watch + two blogs + bare git remotes + failure/recovery
```
