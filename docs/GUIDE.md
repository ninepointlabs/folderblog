# The folderblog guide

This guide shows you how to use folderblog, one small step at a time. You do not need
to know how websites work. If you can save a file into a folder, you can run a blog.

You can read this guide three ways:

- here, on GitHub
- in your terminal: `folderblog guide`
- as a manual page: `man folderblog`

## The big idea

A **blog** is a website where you write posts. Most blog tools make you click lots of
buttons. folderblog is different. **A folder is a blog.**

1. You write a post in a plain text file.
2. You save it into your blog's folder.
3. A few seconds later, it is on the internet.

That's it. There is no "publish" button. Saving *is* publishing.

folderblog runs quietly in the background, like a helper who watches your folders. When
you save something, the helper notices, builds your website, and puts it online.

## Words you will see

- **Terminal**: a window where you type commands. On most Linux computers you can open
  one by searching for "Terminal".
- **Command**: a line you type into the terminal and then press Enter. In this guide,
  commands look like this: `folderblog status`
- **Folder**: a place on your computer that holds files. Some people call it a
  "directory".
- **`~`**: a short way to write *your home folder*. `~/Blogs` means "the Blogs folder
  inside my home folder".
- **Markdown**: a simple way to write text with headings, bold words and links. A
  Markdown file's name ends in `.md`. You will learn it below. It is easy.
- **Post**: one blog entry, like a diary page. Posts have dates.
- **Page**: a page that is not a post, like "About me". Pages do not need dates.
- **GitHub**: a website that stores code and can host websites for free. Its free
  website service is called **GitHub Pages**.
- **Publish** or **deploy**: put your website on the internet.
- **Build**: turn your Markdown files into a real website.

## Step 1: Install folderblog

Pick the one line that matches your computer, and type it in the terminal. You may be
asked for your password. That is normal.

**Arch Linux, Omarchy or Manjaro:**

```sh
curl -LO https://github.com/ninepointlabs/folderblog/releases/latest/download/folderblog-bin-0.2.0-1-x86_64.pkg.tar.zst
sudo pacman -U folderblog-bin-0.2.0-1-x86_64.pkg.tar.zst
```

**Debian, Ubuntu, Pop!_OS or Mint:**

```sh
curl -LO https://github.com/ninepointlabs/folderblog/releases/latest/download/folderblog_0.2.0-1_amd64.deb
sudo apt install ./folderblog_0.2.0-1_amd64.deb
```

**Fedora, RHEL or openSUSE:**

```sh
sudo dnf install https://github.com/ninepointlabs/folderblog/releases/latest/download/folderblog-0.2.0-1.x86_64.rpm
```

**Any other Linux:**

```sh
curl -L https://github.com/ninepointlabs/folderblog/releases/latest/download/folderblog-0.2.0-x86_64-linux.tar.gz | tar xz
sudo install -Dm755 folderblog-0.2.0-x86_64-linux/folderblog /usr/local/bin/folderblog
```

Now check that it worked:

```sh
folderblog --version
```

It should say `folderblog 0.2.0` (or a newer number).

folderblog also needs a program called `git`. Most computers already have it. To pop up
little messages on your screen, it uses `notify-send`, which most computers also have.

## Step 2: Get GitHub ready (only once)

This step takes about five minutes, and you only ever do it once. After this, folderblog
can make new websites for you all by itself.

You will make your own small **GitHub App**. Think of it as a key that only you hold. It
lets folderblog make websites on your GitHub account, and nothing else.

If you don't have a GitHub account yet, make one at <https://github.com/signup>.

### 2a. Make the app

Open this page: <https://github.com/settings/apps/new>

Fill in the form like this:

| Box on the form | What to put |
|---|---|
| GitHub App name | Any name nobody else has used, like `folderblog-yourname` |
| Homepage URL | Anything, like `https://github.com/yourname` |
| Callback URL | Leave it empty |
| Expire user authorization tokens | Leave it **ticked** |
| Request user authorization (OAuth) during installation | Leave it **unticked** |
| Enable Device Flow | **Tick it.** This is important |
| Webhook, Active | **Untick it** |

Further down is **Repository permissions**. Change these four:

| Permission | Set it to |
|---|---|
| Administration | Read and write |
| Contents | Read and write |
| Pages | Read and write |
| Metadata | Read-only (it picks this by itself) |

Leave every **Account permission** at "No access".

Where it asks **Where can this GitHub App be installed?**, pick **Only on this account**.

Click **Create GitHub App**. A new page opens. Find the **Client ID**. It looks like
`Iv23li` followed by more letters. Copy it. You do **not** need a "client secret" or a
"private key", so don't make those.

### 2b. Install the app on your account

On your app's page, click **Install App** on the left. Then click **Install** next to
your name. Choose **All repositories** and click **Install**.

### 2c. Log in

In the terminal, type this, but put your own Client ID at the end:

```sh
folderblog login github --client-id Iv23li...
```

folderblog shows you a short code, like `4DC2-EA36`. Open
<https://github.com/login/device>, type the code, and click to approve. Done!

You only type `--client-id` the very first time. folderblog remembers it.

Your login lasts about six months, and it renews itself while you keep using folderblog.
If it ever runs out, folderblog tells you a week before, and you just run
`folderblog login github` again.

> **Keep it safe.** Your login is saved in `~/.config/folderblog/github.json`. Only you
> can read that file. Treat it like a house key. It can make and delete websites on
> your GitHub account.

## Step 3: Start the helper

The helper is the part that watches your folders. Start it once:

```sh
folderblog install-service
```

Now it runs all the time, even after you restart your computer. You never need to start
it again.

## Step 4: Make a blog

Pick a short name with no spaces, like `myblog`. Then type:

```sh
folderblog new myblog
```

This makes a new folder, `~/Blogs/myblog`. Because you logged in to GitHub, folderblog
also gets your website ready. The first time you publish, it makes the GitHub
repository and turns on GitHub Pages for you.

Your blog will live at `https://yourname.github.io/myblog/`. (Put your GitHub name where
it says `yourname`.)

Here is what is inside your new blog folder:

```
~/Blogs/myblog/
  blog.toml     the blog's settings: its title, its address, where it goes
  posts/        your posts go here
  pages/        pages like "About me" go here
  drafts/       posts that aren't ready yet (only you can see them)
  static/       files to put online exactly as they are
  data/         lists and facts that your design can use
  theme/        the design: how your blog looks
  AGENTS.md     notes for AI coding helpers (made for you)
  .blog/        folderblog's own scratch space; leave it alone
```

If you want a photo gallery too, see "Photo gallery" below. It adds a `gallery/` folder.

## Step 5: Write your first post

Open any text editor. Type this:

```markdown
# Hello, world

This is my very first post.
```

Save it as `hello.md` inside `~/Blogs/myblog/posts/`.

That's it! In about a minute your post is on the internet. (The very first time can
take one or two minutes, because GitHub sets things up. After that, posts appear in
about 30 to 60 seconds.)

To see what the helper is doing, type:

```sh
folderblog status
```

## Writing in Markdown

Markdown is plain text with a few special marks:

```markdown
# A big heading
## A smaller heading

A normal paragraph. Leave an empty line between paragraphs.

**bold words** and *slanted words*

- a list item
- another list item

1. a numbered item
2. the next one

[a link](https://example.com)

![a picture](cat.jpg)

> a quote from someone

`a bit of code`
```

You can also make tables, footnotes, checklists and code blocks with colours. If you
know HTML, you can type HTML right into a post too.

## How folderblog understands your post

You don't have to tell folderblog anything. It works things out by itself.

**The title.** The first line that starts with `#` becomes the title. folderblog takes
it out of the post so it isn't shown twice. No heading? Then the file name becomes the
title: `my-summer.md` becomes "My summer".

**The date.** If your file name starts with a date, like `2026-09-30-my-summer.md`, that
is the post's date. If it doesn't, the date is the moment folderblog first saw the file.
folderblog writes that moment down and never changes it, even if you edit the post later.

**The web address.** The address comes from the file name. `my-summer.md` written in
September 2026 lives at `/2026/09/my-summer/`. You can change this pattern in
`blog.toml` (see "Settings").

**The preview text.** On the front page, folderblog shows the start of each post: the
first paragraph. To choose where the preview stops, put this line in your post:

```markdown
<!--more-->
```

Everything above that line is the preview.

## Extra details at the top of a post (front matter)

You can add a few details at the very top of a post, between two `---` lines. This is
called **front matter**. It is always optional.

```markdown
---
title: A walk in the woods
date: 2026-03-21
tags: [walking, nature]
---
The post starts here.
```

Here is every detail folderblog knows about:

| Detail | What it does |
|---|---|
| `title` | The title, if you don't want to use the first heading |
| `date` | The date, like `2026-03-21` or `2026-03-21 14:30` |
| `tags` | Labels, like `[walking, nature]` or `walking, nature` |
| `slug` | The last part of the web address, like `woods-walk` |
| `url` | The whole web address, like `/walks/woods/` |
| `layout` | Use a different design for this one post (see "Changing how it looks") |
| `draft` | `true` keeps it off the real website |
| `excerpt` or `summary` | Your own preview text |
| `updated` | When you last changed it, like `2026-04-01` |
| `weight` | For pages: a smaller number comes first in the menu |

You can add **any other detail you like**, such as `mood: happy`. A design can show it.

You can write front matter in another style called TOML, between `+++` lines, if you
prefer:

```markdown
+++
title = "A walk in the woods"
tags = ["walking", "nature"]
+++
```

If folderblog can't understand the front matter or a date (maybe there's a typo), it
stops and tells you. Your website stays exactly as it was until you fix it.

## Tags

Tags are labels that group posts together:

```markdown
---
tags: [cooking, summer]
---
```

Each tag gets its own page, like `/tags/cooking/`, that lists every post with that tag.

## Pictures in posts

There are two easy ways.

**Way 1: give the post its own folder.** Make a folder inside `posts/`, put the post in
it as `index.md`, and put the pictures next to it:

```
posts/
  beach-day/
    index.md
    sandcastle.jpg
    waves.jpg
```

In `index.md`, write `![My sandcastle](sandcastle.jpg)`. The folder name, `beach-day`,
becomes the web address.

**Way 2: keep pictures anywhere in `posts/` or `static/`,** and point to them. For
example, a picture saved as `posts/images/dog.jpg` can be shown with
`![My dog](images/dog.jpg)` from a post in `posts/`.

## Pages

Pages are for things that don't change much, like "About me".

- `pages/about.md` becomes `/about/`
- `pages/projects/robot.md` becomes `/projects/robot/`
- `pages/index.md` becomes your front page, if you want to replace the list of posts

Pages show up in the menu at the top of the default design. Use `weight` in the front
matter to choose their order.

A page can also be an `.html` file. Then it is its own designed page.

## Drafts

Not finished yet? Save it in `drafts/` instead of `posts/`. Drafts never go on the real
website. You can still see them with `folderblog preview` (see below).

When it is ready, move the file into `posts/`. Its date becomes the day you moved it.

You can also hide a post that's already in `posts/` by adding `draft: true` to its
front matter.

## Taking a post down

Delete the file, or move it back into `drafts/`. It disappears from the website at the
next publish.

## The static folder

Anything in `static/` is put online exactly as it is, at the same place. For example,
`static/favicon.ico` becomes `/favicon.ico`, and `static/files/recipe.pdf` becomes
`/files/recipe.pdf`. Use it for the little icon in the browser tab, downloads, and so on.

## The data folder

Files in `data/` hold lists and facts that your design can use. They can be JSON, TOML
or YAML files. For example, `data/links.toml` becomes `data.links` in the design. This
is handy for things like a list of favourite books.

## Photo gallery

folderblog can make a photo gallery for your blog. Turn it on once:

```sh
folderblog gallery on ~/Blogs/myblog
```

(If you are already inside the blog's folder in the terminal, you can just type
`folderblog gallery on`.)

This makes a `gallery/` folder. **Put pictures in it, and they appear on your website**
at `/gallery/`. Each photo also gets its own page.

```
gallery/
  index.md                optional: a title and a hello for the gallery page
  sunset.jpg              a photo; nothing else needed
  sunset.md               optional: a title, caption and tags for sunset.jpg
  2026-06-01-picnic.jpg   a date at the start of the name sets the photo's date
  camping-trip/           a folder inside gallery/ is an album
    index.md              optional: the album's title, date, tags and cover
    tent.jpg
    campfire.jpg
```

**Albums.** Every folder inside `gallery/` is an album. Its pictures show in order, oldest
first, like a story.

**Titles, captions and tags.** Make a Markdown file with the **same name** as the photo:
`sunset.md` next to `sunset.jpg`. (You can also call it `sunset.jpg.md`.)

```markdown
---
tags: [sky, evening]
alt: An orange sky over the sea
---
# Sunset at the beach

The whole sky turned orange for about ten minutes.
```

- The heading is the photo's title.
- The words underneath are the caption.
- `tags` are labels. Each tag gets its own page, like `/gallery/tags/sky/`.
- `alt` describes the picture for people who can't see it. It's kind to add one.
- You can also use `title`, `date`, `slug` and `draft: true` (to hide a photo).

Photos straight from a phone or camera (like `IMG_2041.jpg`) don't get a title from their
file name, because those names aren't nice titles.

**Album details.** An album's `index.md` can have:

```markdown
---
title: Our camping trip
date: 2026-07-04
tags: [camping]
cover: campfire.jpg
---
Three nights by the lake.
```

- `tags` here are given to **every** photo in the album.
- `cover` picks which photo shows on the album's card.
- `order: newest` shows the newest photo first instead.
- The words underneath describe the album.

**The gallery's own title.** `gallery/index.md` can say `# Photographs` and a sentence
or two. That becomes the gallery page's title and introduction.

**Dates.** A photo's date comes from its `.md` file, or else a date at the start of its
file name, or else **the moment the camera took it**, or else when folderblog first saw it.

**Looking at photos.** Click a photo on the website and it opens big, on a dark screen.
Use the arrow keys (or swipe on a phone) to see the next one. Press Escape to close it.

**Your privacy.** Phones secretly save *where* each photo was taken. folderblog never
puts your original photos online. It makes smaller copies (2048 pixels across, plus a
little thumbnail) that don't carry that hidden information. Making the copies happens
once per photo, so later builds are fast.

**Which pictures work.** JPEG, PNG, WebP and GIF. iPhone HEIC photos and camera RAW
files don't work on websites, so folderblog skips them and tells you. Save them as JPEG
first.

**The menu link.** In the standard design, a "Gallery" link appears in the menu as soon
as the gallery has at least one photo.

**Turning it off.** `folderblog gallery off` hides the gallery. Your photos stay safe in
the folder.

## See it before everyone else (preview)

Want to look at your blog before it's online? Type:

```sh
cd ~/Blogs/myblog
folderblog preview
```

Then open <http://127.0.0.1:4000> in your web browser. This preview:

- shows your **drafts** too
- **reloads by itself** every time you save a file
- is only on your computer. Nobody else can see it.

Press `Ctrl` and `C` together in the terminal to stop it.

## Is everything OK?

**Type `folderblog status`.** It shows every blog: when it last built, when it last
published, and whether GitHub has finished putting it online. If anything is broken, it
says what and where.

**Watch for pop-up messages.** If something goes wrong, a message pops up on your screen.
When it's fixed, another message says so.

**Nothing breaks your website.** folderblog builds each new version in a separate place.
It only swaps it in if *everything* worked. If you make a mistake, your website stays
exactly as it was until you fix it.

**Offline? No problem.** If folderblog can't publish (no internet, or your login ran
out), it tries again every 5 minutes. Everything waiting goes out by itself once things
work again.

**Want every detail?** The helper writes a diary called a log:

```sh
journalctl --user -u folderblog -f
```

## Your own web address (custom domains)

You can use your own address, like `example.com`, instead of the github.io one.

**For a new blog:**

```sh
folderblog new example.com --domain example.com
```

**For a blog you already have:** open its `blog.toml`. Under `[deploy]`, add a line
`domain = "example.com"`, and change `base_url` at the top to `"https://example.com"`.

Then go to the company where you bought your domain, and add these records (folderblog
prints them for you when you use `--domain`):

- For an address like `blog.example.com`: one **CNAME** record pointing to
  `yourname.github.io`
- For an address like `example.com`: four **A** records: `185.199.108.153`,
  `185.199.109.153`, `185.199.110.153` and `185.199.111.153`

Changes to these records can take a few minutes or a few hours to work. folderblog
switches on the padlock (HTTPS) as soon as GitHub is ready.

Good idea: verify your domain in your GitHub settings, so nobody else can use it with
GitHub Pages.

## Publishing somewhere other than GitHub Pages

GitHub Pages is the easiest, but you can publish almost anywhere. Change the
`[deploy]` part of `blog.toml`.

**To any git place**, like a server or another code website:

```toml
[deploy]
type = "git"
remote = "git@github.com:you/site.git"
branch = "gh-pages"
```

**With any command you like**, such as `rsync`:

```toml
[deploy]
type = "command"
command = 'rsync -a --delete "$FOLDERBLOG_OUT/" me@server:/var/www/blog/'
```

`$FOLDERBLOG_OUT` is the folder with your finished website in it.

**Nowhere at all:** make a blog with `folderblog new myblog --no-github`, and leave out
`[deploy]`. folderblog still builds the website into `.blog/public` inside the blog.

folderblog only publishes when your website actually changed.

## Settings (blog.toml)

Each blog has a settings file called `blog.toml`. Here is every setting:

```toml
title = "Field Notes"                         # your blog's name (needed)
base_url = "https://yourname.github.io/myblog" # where the blog lives (needed)
description = "Notes from my workbench"       # a sentence about your blog
author = "Your Name"                          # who writes it
language = "en"                               # the language, like en, fr, de
permalink = "/{year}/{month}/{slug}/"         # the pattern for post addresses

[feed]
limit = 20                                    # how many posts go in the RSS feed

[gallery]
enabled = true                                # the photo gallery is on
url = "/gallery/"                             # where the gallery lives

[deploy]
type = "github"                               # github, git or command
owner = "yourname"                            # github: your GitHub name
repo = "myblog"                               # github: the repository name
# branch = "gh-pages"                         # github and git: which branch
# domain = "example.com"                      # github: your own web address
# private = false                             # github: a private repository

[build]
pre = []                                      # commands to run before building
post = []                                     # commands to run after building

[watch]
ignore = []                                   # folders to never react to

[params]                                      # anything you like, for your design
tagline = "Small notes, often"
```

Some more about a few of them:

- **`base_url`** must be the full address, including the part after the `.io`, like
  `/myblog`. If it's wrong, links on your website break.
- **`permalink`** can use `{year}`, `{month}`, `{day}` and `{slug}`. `{slug}` must be in
  it. Changing it changes the address of every post.
- **`[deploy]` for `git`** also has `nojekyll` (default `true`), `author_name` and
  `author_email`.
- **`[build]`** commands are for people who use extra tools, like Tailwind. They run
  inside the blog folder. They only ever come from `blog.toml`, never from a design, so
  a design you copied can't run programs on your computer.
- **`[params]`** is a free space. Anything you put there, your design can use.

The newest, complete list is always in the blog's own `AGENTS.md` file.

## Changing how it looks

The design of your blog lives in the `theme/` folder. Every new blog starts with a clean,
easy-to-read design. You can change anything in it. It is all yours.

**The easy way: ask an AI coding helper.** Open the blog folder with a coding helper
(like Claude Code) and ask for what you want, for example:

> Read AGENTS.md, then make this blog look like an old newspaper.

`AGENTS.md` explains everything the helper needs to know, and how it can check its own
work.

**The do-it-yourself way.** Designs are made of **templates**: HTML files with little
`{{ }}` and `{% %}` instructions in them. A few rules:

- `.html` files in `theme/` become pages at the same place.
- Files ending in `.jinja` become other kinds of files, like `style.css.jinja` becoming
  `style.css`.
- Everything else (pictures, styles) is copied as it is.
- Files and folders whose names start with `_` (like `_layouts/`) are helpers. They are
  never put online by themselves.
- Each post uses `theme/_layouts/post.html`. A post with `layout: photo` in its front
  matter uses `theme/_layouts/photo.html` instead.
- The photo gallery uses the templates in `theme/_gallery/`.

These commands help while designing:

```sh
folderblog preview                    # see it live while you work
folderblog preview --fixtures stress  # try your design with very tricky sample posts
folderblog check                      # find mistakes, broken links and problems
folderblog data /2026/09/hello/       # see every fact a page's template can use
```

## Every command, in detail

You can put the blog's folder after most commands. If you leave it out, folderblog uses
the folder you are in right now. So these two do the same thing:

```sh
folderblog check ~/Blogs/myblog
cd ~/Blogs/myblog && folderblog check
```

Every command has its own short help: add `--help`, like `folderblog new --help`.

### folderblog new

Makes a new blog folder.

```sh
folderblog new NAME [--domain DOMAIN] [--repo REPO] [--no-github] [--root FOLDER]
```

- `NAME` is the blog's name. The new folder is `~/Blogs/NAME`. You can also give a full
  path, like `~/sites/myblog`.
- `--domain DOMAIN` uses your own web address, like `--domain example.com`, and prints the
  records to add for it.
- `--repo REPO` picks the GitHub repository's name. Normally it is the blog's name.
- `--no-github` makes a blog that doesn't publish to GitHub, even if you are logged in.
- `--root FOLDER` makes the blog in a different folder than `~/Blogs`.

Special trick: `folderblog new yourname.github.io` makes your main GitHub website, at
`https://yourname.github.io/`.

### folderblog login

Connects folderblog to your GitHub account. See Step 2.

```sh
folderblog login github [--client-id ID]
```

- `--client-id ID` is your GitHub App's Client ID. You only need it the first time.

### folderblog logout

Makes folderblog forget your GitHub login.

```sh
folderblog logout github
```

To also take away the app's permission on GitHub, go to GitHub, then **Settings**, then
**Applications**, then **Authorized GitHub Apps**, and click **Revoke**.

### folderblog install-service

Starts the helper that watches your blogs, and makes it start every time you turn on
your computer.

```sh
folderblog install-service [--root FOLDER] [--no-start] [--print]
```

- `--root FOLDER` watches a different folder than `~/Blogs`.
- `--no-start` sets everything up but doesn't start it yet.
- `--print` only shows the setup file. It doesn't change anything.

### folderblog status

Shows how every blog is doing.

```sh
folderblog status [--json] [--root FOLDER]
```

- `--json` prints the same facts in a form that other programs can read.
- `--root FOLDER` looks in a different folder than `~/Blogs`.

It finishes with code 1 if anything is broken, so scripts and status bars can use it.

### folderblog preview

Shows your blog on your own computer, with drafts, and reloads when you save.

```sh
folderblog preview [BLOG] [--port NUMBER] [--fixtures SET]
```

- `--port NUMBER` uses a different port. Normally the address is
  <http://127.0.0.1:4000>, and `--port 5000` makes it <http://127.0.0.1:5000>.
- `--fixtures SET` shows your design with sample posts instead of your own. The sets
  are `stress` (long titles, code, tables, pictures, lots of tags), `empty` (no posts at
  all) and `one` (just one post). `--fixtures` alone means `stress`.

### folderblog check

Builds everything and looks for problems, without publishing.

```sh
folderblog check [BLOG] [--fixtures SET]
```

It finds mistakes in templates (and tells you the file and line), checks the RSS feed,
looks for links that go nowhere, and warns about things like a placeholder `base_url`.
It finishes with code 1 if it found errors.

- `--fixtures SET` checks your design with sample posts. `--fixtures` alone means
  `stress`.

### folderblog build

Builds the website without publishing it.

```sh
folderblog build [BLOG] [--out FOLDER] [--drafts] [--fixtures SET]
```

Normally it builds into `.blog/public` inside the blog.

- `--out FOLDER` builds into another folder, so you can look at the files.
- `--drafts` includes your drafts.
- `--fixtures SET` builds sample posts instead of your own.

### folderblog deploy

Builds a blog and publishes it **right now**, without waiting for the helper.

```sh
folderblog deploy [BLOG] [--force]
```

- `--force` publishes even if nothing changed since last time. Use it after changing
  where the blog publishes, like adding a `domain`.

### folderblog gallery

Turns the photo gallery on or off, or tells you if it's on.

```sh
folderblog gallery [on|off] [BLOG]
```

- `on` turns it on, makes the `gallery/` folder, and adds the gallery's design files to
  `theme/_gallery/`. It never replaces design files you already have.
- `off` turns it off. Your photos stay where they are.
- Nothing after `gallery` just tells you whether it's on.

### folderblog data

Shows the facts a page's template gets. This is for people (and AI helpers) changing the
design.

```sh
folderblog data [ADDRESS] [--blog BLOG] [--drafts] [--fixtures SET]
```

- With no address, it lists every page and file on the website, and what made each one.
- With an address, like `folderblog data /about/`, it prints every fact that page's
  template can use.
- `--blog BLOG` picks the blog. (Here the blog comes after `--blog`, because the first
  word is the address.)
- `--drafts` includes drafts.
- `--fixtures SET` uses sample posts.

### folderblog agents-md

Updates the blog's `AGENTS.md` file, the notes for AI coding helpers.

```sh
folderblog agents-md [BLOG] [--write]
```

- Without `--write` it only prints the new notes.
- `--write` saves them into `AGENTS.md`. Anything you wrote outside the generated part
  is kept.

Run it after you update folderblog, so the notes match the new version.

### folderblog watch

This is the helper itself. It watches every blog, builds and publishes. You normally
don't run it yourself: `install-service` runs it for you in the background.

```sh
folderblog watch [--root FOLDER]
```

- `--root FOLDER` watches a different folder than `~/Blogs`.

### folderblog guide

Shows this guide in your terminal.

```sh
folderblog guide
```

### folderblog man

Shows this guide as a manual page. The packages also install it, so `man folderblog`
works too. If you send it into a file, like `folderblog man > folderblog.1`, it writes
the manual page's source instead.

### folderblog help

Shows the list of commands. `folderblog help new` shows the help for one command, just
like `folderblog new --help`.

## Special settings for your terminal

These are called **environment variables**. Most people never need them.

| Name | What it does |
|---|---|
| `FOLDERBLOG_ROOT` | Use another folder instead of `~/Blogs` |
| `FOLDERBLOG_NO_NOTIFY` | If set to anything, no pop-up messages |
| `FOLDERBLOG_CONFIG_DIR` | Where to keep the GitHub login (normally `~/.config/folderblog`) |
| `FOLDERBLOG_GITHUB_CLIENT_ID` | A Client ID to log in with, instead of `--client-id` |

To make the background helper watch a different folder, type
`systemctl --user edit folderblog` and add these two lines:

```
[Service]
Environment=FOLDERBLOG_ROOT=/path/to/blogs
```

Commands in `[build]` and `type = "command"` publishing also get `FOLDERBLOG_OUT` (the
finished website folder) and `FOLDERBLOG_BLOG` (the blog's folder).

## When something goes wrong

**Nothing happens when I save.** Type `folderblog status`. If the helper isn't running,
type `folderblog install-service`.

**It says I'm not logged in, or my login expired.** Type `folderblog login github`.
Everything waiting is published within 5 minutes.

**GitHub won't start a login.** Check the Client ID. In your GitHub App's settings, make
sure **Enable Device Flow** is ticked.

**Making the repository fails with "403" or "Resource not accessible by integration".**
Your GitHub App is missing a permission, or isn't installed. Go back to Step 2a and 2b.
After changing an app's permissions, GitHub asks you to approve the change on the app's
install page.

**My new website shows GitHub's "404" page for a few minutes.** That's normal the first
time. GitHub is still building it. `folderblog status` shows when it's done.

**My website has no colours, or links are broken.** Check `base_url` in `blog.toml`. It
must match your website's real address exactly, including the `/myblog` part.

**No pop-up messages.** Install `libnotify` (it gives you `notify-send`).

**There's a mistake in a template.** `folderblog check` tells you the file and the line.
Your website keeps its last good version until you fix it.

**A photo doesn't show up.** HEIC and RAW photos don't work; save them as JPEG. Run
`folderblog check` to see a warning for each skipped file.

## How folderblog keeps you safe

- It only **reads** your writing. It never changes your files.
- Each build happens in a fresh place. A broken build never touches your live website.
- It ignores the temporary files that editors and sync programs leave around.
- Each blog builds on its own, so one broken blog never stops the others.
- Your GitHub login stays on your computer. Only you can read it.
- Your original photos never go online, so their hidden location data stays private.
- Designs can't run programs on your computer. Only your own `blog.toml` can.

## Getting more help

- `folderblog --help` lists every command.
- `folderblog COMMAND --help` explains one command.
- `folderblog guide` and `man folderblog` show this guide.
- The README has more technical details: <https://github.com/ninepointlabs/folderblog>
- The website has a video tour: <https://folderblog.ninepointlabs.com>
