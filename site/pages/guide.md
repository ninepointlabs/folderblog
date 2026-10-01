---
title: The guide
description: Install folderblog, connect GitHub once, and start publishing.
weight: 1
---

This is the short version. The [README](https://github.com/ninepointlabs/folderblog#readme) has every detail, and every blog's own `AGENTS.md` documents theming completely.

## 1. Install

Packages for Arch, Debian/Ubuntu and Fedora, plus a static binary for any x86_64 Linux, are on the [releases page](https://github.com/ninepointlabs/folderblog/releases/latest). The [home page](/#install) has the one-line commands.

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

For your own domain: `folderblog new example.com --domain example.com` prints the DNS records to add.

## 4. Write

- `posts/` holds posts: a Markdown file, or a folder with `index.md` and its images.
- `pages/about.md` becomes `/about/`.
- `drafts/` shows only in `folderblog preview`.
- `static/` is copied as-is.
- Delete a file to unpublish it.

Front matter is optional: `title`, `date`, `tags`, `layout` and anything else you like, which themes can read.

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

## 5. Make it yours

A theme is the `theme/` folder in the blog. Files ending in `.html` or `.jinja` are templates rendered to the same path, and everything else is copied. A route header makes one template produce many pages:

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
folderblog data /2026/09/hello/       # the exact template context for a page
```
