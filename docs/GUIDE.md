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
curl -LO https://github.com/ninepointlabs/folderblog/releases/download/v0.2.9/folderblog-bin-0.2.9-1-x86_64.pkg.tar.zst
sudo pacman -U folderblog-bin-0.2.9-1-x86_64.pkg.tar.zst
```

**Debian, Ubuntu, Pop!_OS or Mint:**

```sh
curl -LO https://github.com/ninepointlabs/folderblog/releases/download/v0.2.9/folderblog_0.2.9-1_amd64.deb
sudo apt install ./folderblog_0.2.9-1_amd64.deb
```

**Fedora, RHEL or openSUSE:**

```sh
sudo dnf install https://github.com/ninepointlabs/folderblog/releases/download/v0.2.9/folderblog-0.2.9-1.x86_64.rpm
```

**Any other Linux:**

```sh
curl -L https://github.com/ninepointlabs/folderblog/releases/download/v0.2.9/folderblog-0.2.9-x86_64-linux.tar.gz | tar xz
sudo install -Dm755 folderblog-0.2.9-x86_64-linux/folderblog /usr/local/bin/folderblog
```

Now check that it worked:

```sh
folderblog --version
```

It should say `folderblog 0.2.9`.

These commands always install version 0.2.9, even after newer versions come out. The
[releases page](https://github.com/ninepointlabs/folderblog/releases/latest) has the
newest one: use the same commands with its file names.

**Updating later.** Install the new package the same way (with the new version's file
name from the releases page), then restart the helper:
`systemctl --user restart folderblog`. When it starts, it brings your blogs up to date by
itself: it adds new sample files (like the `templates/` folder) and refreshes `AGENTS.md`.
It never turns anything on and never changes your design, your settings or your posts. If
you delete a sample file, it stays deleted.

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

**If it says "systemctl --user daemon-reload failed"**, your computer has no background
helper manager for your user. That happens in containers, on some servers, and when you
connect over SSH. You have two choices:

- Run `folderblog watch` in a terminal window (or in `tmux`, so it keeps going after you
  close the window). It does the same job as the helper while it runs.
- Or run `loginctl enable-linger $USER`, log out and in again, and then
  `folderblog install-service` again.

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
  templates/    a sample post and podcast episode to copy (see below)
  AGENTS.md     notes for AI coding helpers (made for you)
  dates.toml    when folderblog first saw each post without a date (made for you; keep it)
  .blog/        folderblog's own scratch space; leave it alone
```

If you want a photo gallery too, see "Photo gallery" below. It adds a `gallery/` folder.
If you want a podcast, see "Start a podcast" below. It adds a `podcast/` folder.

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
folderblog writes that moment down in a file called `dates.toml` in your blog folder and
never changes it, even if you edit the post later. Keep `dates.toml` with your blog: if
you copy the blog to another computer or keep it in git, that file makes sure undated
posts keep their dates and web addresses.

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
| `image` | The picture shown when someone shares the post, like `cover.jpg` (beside the post) or a web address |

You can add **any other detail you like**, such as `mood: happy`. A design can show it.

**A ready-made post to copy.** Your blog's `templates/` folder has `post.md`, a post with
every one of these details filled in and a note on each. Copy it into `posts/`, rename it,
change what you need and delete the lines you don't. It starts with `draft: true`, so
delete that line when the post is ready. (Older blogs can get the folder with
`folderblog templates`.)

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

**Every picture in `posts/` goes online,** even one no post shows yet. `folderblog check`
lists the ones nothing shows, so you can delete a picture you didn't mean to share.

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

Two names are special:

- `static/og-default.png` is the picture shown when someone shares a page that has no
  `image` of its own (on social media or in a chat). 1200 by 630 pixels works well.
- `static/robots.txt` tells search engines what to look at. You don't need one: without
  it, folderblog writes one that lets them see everything and tells them where your
  sitemap is. (If your blog lives in a folder of a bigger site, like
  `https://example.com/blog/`, search engines only read `https://example.com/robots.txt`,
  so add the sitemap line to that one instead.)

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

## Start a podcast

A podcast is one more part of your blog. It gets its own page (`/podcast/`), a page for
each episode, and its own feed that podcast apps (Apple Podcasts, Spotify, Pocket Casts,
Fountain and the rest) can follow. You make the recordings; folderblog does the rest.

Turn it on once:

```sh
folderblog podcast on ~/Blogs/myblog
```

This makes a `podcast/` folder and adds the podcast's design files to `theme/_podcast/`.
It also tells git (if you use it) to leave audio files alone, because they are big and
are kept somewhere else.

```
podcast/
  index.md                        the show's name and a few sentences about it
  cover.jpg                       the show's picture (square, 1400 to 3000 pixels)
  2026-10-30-first-episode/       one folder for each episode
    index.md                      the episode's notes
    episode.mp3                   the recording
    transcript.vtt                optional: what was said, word for word
    chapters.json                 optional: the episode's chapters
```

**The show's name.** It comes from the heading in `podcast/index.md` (or `title` under
`[podcast]` in `blog.toml`). With no name of its own, the show uses your blog's name. A
show with the same name as your blog is called "Your Blog (Podcast)" in its feed, so
podcast apps and feed readers don't show it as a second copy of your blog. Give the show
its own name if you don't want the "(Podcast)".

**An episode.** Make a folder inside `podcast/`. Start its name with the date, like
`2026-10-30-first-episode`. Put the recording inside, and an `index.md` with the notes:

```markdown
---
title: "First episode"
guid: "6f0e2c64-3b1b-4c9e-9d7a-1f6a2a8a6e01"
season: 1
episode: 1
summary: "What this show is about."
---
What we talked about, and links to the things we mentioned.
```

- `guid` is the episode's permanent name tag in podcast apps. Write it once and **never
  change it**. If you leave it out, `folderblog check` suggests one you can copy in (it
  checks draft episodes too, and suggests the same one every time).
- `season` and `episode` are numbers. Both are optional.
- `summary` is the short line apps show under the title.
- `draft: true` keeps the episode to yourself until it's ready (you still see it in preview).
- `explicit: true` marks an episode with grown-up language.
- `episode_type` can be `trailer` or `bonus`. Normal episodes don't need it.
- `persons` lists who is on the episode, like `[{ name: "Sam", role: guest }]`.
- `in_main_feed: true` also lists this episode with your blog posts (see below).

**A ready-made episode to copy.** `templates/podcast-episode/` in your blog folder has an
`index.md` with every episode detail and a note on each, including the rarer ones
(episode art, Lightning tips for one episode, which file to use). Copy the folder into
`podcast/`, name it after the episode, put the MP3 beside `index.md`, and fill it in.

**Which recordings work.** MP3 for audio. MP4 for video podcasts. Other kinds (like WAV,
FLAC or M4A) don't work yet; save the episode as MP3 first. folderblog works out how long
each episode is and how big it is by itself. Very large files get a gentle warning, since
they are slow for listeners.

**Your recordings stay with you.** The audio never goes into your blog's website or into
git. Your folder is the master copy. The website and the feed point at the copy on your
audio host (see below).

**The show's picture.** Put a square JPEG or PNG called `cover.jpg` (or `cover.png`) in
`podcast/`. Podcast apps want it between 1400 and 3000 pixels on each side.

**Show settings.** In `blog.toml`, under `[podcast]`:

```toml
[podcast]
enabled = true
category = "Technology"          # one of Apple's podcast categories
author = "Your Name"
owner_email = "you@example.com"  # podcast apps show this; use an address you don't mind sharing
explicit = false
```

`folderblog check` tells you if the category isn't one Apple knows, and lists the ones it does.

**Tips with Lightning (optional).** Listeners in apps like Fountain can send you small
Bitcoin tips while they listen. To turn it on, add your Lightning address:

```toml
lightning_address = "you@rizful.com"
```

New to Lightning? [Rizful](https://rizful.com) is an easy way to get an address. Leave the
line out and nothing changes.

**Also on the blog's front page.** Episodes stay on `/podcast/` and in the podcast feed.
To list them with your posts too (on the home page and in the blog's RSS feed), set
`in_main_feed = true` under `[podcast]`, or `in_main_feed: true` on one episode.

**Where the audio lives.** Podcast apps download each episode from an audio host.
folderblog sets one up for you on Cloudflare R2, and uploads each episode when your blog
publishes. Run:

```sh
folderblog podcast setup ~/Blogs/myblog
```

It walks you through everything, one step at a time, and opens each page in your browser:

1. **A Cloudflare account.** If you don't have one, it opens the sign-up page. Make an
   account and click the link in the email Cloudflare sends you.
2. **Turning on R2.** R2 is free for up to 10 GB stored each month, and downloads are
   always free. (After that it costs about 1.5 cents per GB each month.) Cloudflare does
   ask for a card or PayPal to turn R2 on, even on the free plan.
3. **Connecting folderblog.** The quick way (recommended): folderblog opens a ready-made
   form for one API token. Click *Continue to summary*, then *Create Token*, and paste the
   token when asked (it won't show as you paste). folderblog then makes the bucket and the
   public address for you. That token can manage every R2 bucket in your account. If you'd
   rather it couldn't, choose the careful way: you make the bucket and a key for just that
   bucket yourself, and paste the Account ID, Access Key ID, Secret and public address.
4. **Tips (optional).** It asks for a Lightning address, if you want one.
5. **A check.** It uploads a tiny test file, finds it again, downloads it from the public
   address and removes it, telling you in plain words what worked. If something doesn't,
   it says what usually fixes it, and nothing is saved, so you can simply run it again.

**Your own address, or r2.dev.** You can start with a free `r2.dev` address. It works, but
it is rate-limited and Cloudflare says it isn't meant for production. Your own address,
like `media.yourblog.com`, is better for a real show. It needs that domain to be on
Cloudflare in the same account (if your domain is somewhere else, such as Porkbun, that
means moving its nameservers to Cloudflare first). You can switch later by running setup
again: podcast apps keep following the show, because its `guid` never changes.

**Renaming your blog folder.** Episode files live in a folder on the audio host, named in
`blog.toml` as `prefix` under `[podcast.storage]` (setup fills it in). Blogs set up with an
older folderblog may not have that line; then the folder is named after your blog folder,
and renaming the blog folder would give every episode a new address. `folderblog check`
reminds you to add the line, with the right name already filled in.

**Where your keys go.** The key folderblog needs is saved in
`~/.config/folderblog/storage/<bucket>.json`, which only you can read. It is never put in
`blog.toml`, your blog folder or git. `folderblog podcast setup --show` shows what is set
up, with only the last four letters of the key.

**When an upload fails.** If an episode can't be uploaded (you're offline, say), folderblog
holds back the whole publish, so the feed never points at a missing file. If the helper
(`folderblog install-service`) is running, it tries again by itself every few minutes;
otherwise publish again yourself once you're back online. `folderblog status` says why
it's waiting. Episodes are
uploaded once; saving other files doesn't upload them again.

**Using another host.** If you upload the files yourself, set only
`public_url = "https://..."` under `[podcast.storage]`; `folderblog podcast` lists each
episode's file name on the host, and publishing waits until each file is there.

**Another storage service that speaks S3.** Backblaze B2, Wasabi, MinIO, AWS S3 and many
others work like R2, and folderblog uploads to them for you. `podcast setup` only knows
Cloudflare, so you write the settings yourself:

```toml
[podcast.storage]
type = "s3"
endpoint = "https://s3.us-west-004.backblazeb2.com"   # the service's S3 address
bucket = "myblog-podcast"
region = "us-west-004"                               # if the service asks for one
public_url = "https://media.example.com"             # where listeners download from
```

Then give folderblog the bucket's key, one of two ways:

- Save it in `~/.config/folderblog/storage/myblog-podcast.json` (the bucket's name, then
  `.json`), with only you allowed to read it (`chmod 600`):
  `{"access_key_id": "...", "secret_access_key": "..."}`
- Or set `FOLDERBLOG_STORAGE_KEY_ID` and `FOLDERBLOG_STORAGE_SECRET` (see "Special
  settings for your terminal"). These win if both are there.

**Let an AI helper put it up.** If you use an AI helper such as Claude Code, Cursor or
Claude Desktop, you can connect it to folderblog (see `folderblog mcp` below). Then you
can just say "put this up" and point at a recording. The helper looks at the file, makes
the episode folder, writes the show notes and chapters, fills in the podcast details, and
checks everything. The episode stays a **draft**: nobody sees it until you say yes. The
helper shows you the title, date and notes and asks first; only then does it publish.
(If you'd rather always publish yourself, put `mcp_publish = false` under `[podcast]`; the
helper then gives you the command to run.)

**Publishing an episode yourself.** Remove `draft: true` from its `index.md`, or run
`folderblog podcast publish --episode first-episode ~/Blogs/myblog`, which also checks it,
uploads it and publishes the blog straight away. It changes only the `draft: true` line
(to `draft: false`); everything else in `index.md`, notes included, stays as you wrote it.

**Check before you share it.** `folderblog check` is strict about podcasts: it checks
everything podcast apps need and explains anything missing in plain words. Once your
show is online, you can also paste the feed address into
[validator.truefans.fm](https://validator.truefans.fm) or
[podba.se/validate](https://podba.se/validate/).

**Telling podcast apps.** Your feed's address is `https://<your blog>/podcast/feed.xml`
(`folderblog podcast` shows it). Your podcast page shows it too, in a **Subscribe** box
near the top, ready for listeners to copy. Submit that address once to Apple Podcasts
Connect, Spotify for Creators and the Podcast Index; after that, new episodes show up by
themselves.

**Podcast stats with OP3.** Want to know how many people download your episodes?
[OP3](https://op3.dev) is a free, open-source stats service for podcasts. It works as a
*prefix*: podcast apps ask OP3 for the episode, OP3 counts the download and sends the app
on to your file. To turn it on, add one line under `[podcast]` in `blog.toml`:

```toml
[podcast]
analytics_prefix = "https://op3.dev/e/"
```

That's all. You don't sign up anywhere. On the next build, every episode in your feeds
gets an address like `https://op3.dev/e,pg=<your guid>/media.example.com/myblog/pilot-1a2b3c4d.mp3`.
(folderblog leaves out the `https://` of your own address and adds your show's `guid`, the
way OP3 asks.) Your episode files stay where they are, uploads and checks still go straight
to your host, and the player on your episode pages keeps playing your own address, so
visits to your blog aren't counted as podcast downloads. `folderblog podcast` shows whether
stats are on. Remove the line to turn them off again.

To see your numbers: after OP3 has seen a few days of downloads, your show gets a free,
public stats page at `https://op3.dev/show/<your guid>`. Your guid is the `guid = "..."`
line under `[podcast]` in `blog.toml`. You can also paste your feed address into the form
on [op3.dev/setup](https://op3.dev/setup) to check that OP3 sees it. Other stats services
that work as a prefix work the same way: put their prefix (it must start with `https://`
and end with `/`) in `analytics_prefix`.

**Turning it off.** `folderblog podcast off` hides the podcast. Your episodes stay safe in
the folder.

## See it before everyone else (preview)

Want to look at your blog before it's online? Type:

```sh
cd ~/Blogs/myblog
folderblog preview
```

Then open the address it prints in your web browser, normally <http://127.0.0.1:4000>.
(If your `base_url` has a folder in it, like `/myblog`, the address includes that folder;
opening the plain address takes you there too.) This preview:

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

GitHub Pages is the easiest, but you can publish almost anywhere. Two changes in
`blog.toml`:

1. **Set `base_url`** at the top to the address where your website will live, like
   `base_url = "https://blog.example.com"`. A blog made with `--no-github` (or while you
   weren't logged in to GitHub) starts with a made-up address like
   `https://myblog.example.com`, and links in your feed and sitemap would point there.
   `folderblog check` warns you while it's still the made-up one.
2. **Change the `[deploy]` part**, in one of these ways.

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
`[deploy]`. folderblog still builds the website into `.blog/public` inside the blog. If
you copy that folder somewhere by hand later, set `base_url` to that address first.

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
image = "/img/share.png"                      # the picture for shared links (default: static/og-default.png)

[feed]
limit = 20                                    # how many posts go in the RSS feed

[gallery]
enabled = true                                # the photo gallery is on
url = "/gallery/"                             # where the gallery lives

[podcast]
enabled = true                                # the podcast is on
category = "Technology"                       # an Apple Podcasts category (needed)
author = "Your Name"                          # who makes the show
owner_email = "you@example.com"               # shown to podcast apps
# url = "/podcast/"                           # where the show lives
# lightning_address = "you@rizful.com"        # optional tips with Lightning
# in_main_feed = false                        # also list episodes with your posts
# analytics_prefix = "https://op3.dev/e/"    # download stats with OP3 (see above)
# guid = "..."                                # set for you once; never change it

[podcast.storage]
type = "r2"                                   # set by `folderblog podcast setup`
account_id = "..."                            # your Cloudflare Account ID
bucket = "myblog-podcast"                     # the R2 bucket
public_url = "https://media.example.com"      # where podcast apps download episodes

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

[analytics]                                   # optional, off unless you add it
goatcounter = "https://stats.example.com"     # your GoatCounter address
# host_prefix = true                          # count pages as "yourblog.com/page"

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
- **`deploy_branch`** is for blogs whose folder is also a git repository. Put
  `deploy_branch = "main"` at the top (above `[feed]`). folderblog then only publishes while
  the folder is on the `main` branch and git isn't in the middle of a merge or rebase. While
  it waits, `folderblog status` says why it is held, and it publishes by itself within a few
  minutes once you're back.
- **`[analytics]`** is optional and off unless you add it. See *Counting visitors* below.
- **`[params]`** is a free space. Anything you put there, your design can use.

The newest, complete list is always in the blog's own `AGENTS.md` file.

## Counting visitors (optional)

Want to know how many people read your posts? folderblog can add
[GoatCounter](https://www.goatcounter.com), a simple visitor counter. It is **off unless you
turn it on**. GoatCounter uses no cookies and keeps no personal data about your readers. You
can run it yourself or use the hosted one at goatcounter.com.

Open `blog.toml` and add, with your own GoatCounter address:

```toml
[analytics]
goatcounter = "https://stats.example.com"
```

The address must start with `https://`. From the next publish on, every page of your blog
counts a visit, whichever design you use. Things to know:

- Each visit is counted as `yourblog.com/the-page`, so one GoatCounter can count several blogs
  and still tell them apart. To count just `/the-page`, add `host_prefix = false`.
- Someone landing on a missing page isn't counted as a visit. It shows up in GoatCounter as an
  event named `404 yourblog.com/the-missing-page`, so you can spot broken links.
- `folderblog preview` never counts, so looking at your own blog doesn't add visits.
- Files in the `static` folder are put online untouched, so they are not counted.
- `folderblog check` tells you if the address is wrong.
- To turn it off again, delete the `[analytics]` lines.

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

- `.html` files in `theme/` become pages at the same place, unless the file starts with
  a note saying where it goes. The standard `theme/archive.html` starts like this, so it
  becomes `/archive/` instead of `/archive.html`:

  ```
  {#---
  url = "/archive/"
  ---#}
  ```

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
folderblog data                      # list every page of your blog
folderblog data /archive/             # see every fact that page's template can use
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

### folderblog podcast

Turns the podcast on or off, or shows the show, its episodes and anything that needs fixing.

```sh
folderblog podcast [on|off] [BLOG]
```

- `on` turns it on, makes the `podcast/` folder, adds the podcast's design files to
  `theme/_podcast/` (never replacing ones you have), and adds audio files to `.gitignore`.
  The first time, it also gives the show its permanent `guid` in `blog.toml`.
- `off` turns it off. Your episodes stay where they are.
- Nothing after `podcast` lists the episodes, the feed address, whether download stats
  (`analytics_prefix`) are on, and any problems.
- `setup` connects audio hosting on Cloudflare R2, step by step (see "Start a podcast").
- `publish --episode SLUG` publishes a draft episode now: it checks it, uploads the audio,
  and publishes the blog. If anything is wrong, it stays a draft and you're told why.

For scripts and AI helpers, `setup` also runs without questions. Secrets are only ever
read from stdin, never typed after the command, so they don't end up in your shell history:

```sh
# Quick: folderblog makes the bucket (a Cloudflare API token on stdin).
folderblog podcast setup --account-id ID --token-from-stdin [--bucket NAME] < token.txt
#   --domain media.example.com --zone-id ZONE   connects your own address instead of r2.dev
# Careful: you made the bucket and its key yourself.
folderblog podcast setup --account-id ID --bucket NAME --public-url https://... \
  --access-key-id KEY --secret-from-stdin < secret.txt
```

- `--jurisdiction eu` (or `fedramp`) is for buckets made in that jurisdiction.
- `--lightning-address you@rizful.com` adds tips.
- `--no-browser` prints the links instead of opening them.
- `--show` shows the current settings, with only the last four letters of the key.

### folderblog mcp

Lets an AI helper (Claude Code, Cursor, Claude Desktop and others that speak MCP) work on
your podcast. You don't run this yourself; your helper starts it. Tell your helper about it
once, for example in Claude Code:

```sh
claude mcp add folderblog -- folderblog mcp
```

```sh
folderblog mcp [--root FOLDER]
```

- `--root` is the folder that holds your blogs (default `~/Blogs`), so you can say "my
  blog example.com" instead of a full path.

The helper can see the show and its episodes, look at a recording, make a **draft**
episode, change its details, write chapters and run the checks. Publishing is the only
thing that changes your public site, and it needs your yes. folderblog never gives the
helper your storage keys, and only ever writes inside your blog's `podcast/` folder.

### folderblog data

Shows the facts a page's template gets. This is for people (and AI helpers) changing the
design.

```sh
folderblog data [ADDRESS] [--blog BLOG] [--drafts] [--fixtures SET]
```

- With no address, it lists every page and file on the website, and what made each one.
- With an address, like `folderblog data /archive/`, it prints every fact that page's
  template can use. Use an address from your own blog (the list above shows them all);
  one that doesn't exist is an error.
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

The helper does this by itself when it starts after an update. Run it yourself if
you don't use the helper.

### folderblog templates

Adds the sample files to the blog's `templates/` folder: `post.md` and
`podcast-episode/index.md`, with every detail filled in. New blogs already have them.

```sh
folderblog templates [BLOG]
```

It only adds files that are missing. If you changed a template, it stays as you left it.
The helper also adds missing templates by itself the first time it starts after an update.
The `templates/` folder is never put on your website.

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
| `FOLDERBLOG_CONFIG_DIR` | Where to keep the GitHub login and storage keys (normally `~/.config/folderblog`) |
| `FOLDERBLOG_GITHUB_CLIENT_ID` | A Client ID to log in with, instead of `--client-id` |
| `FOLDERBLOG_STORAGE_KEY_ID` | The access key ID for podcast audio storage (with the next one); used instead of the saved key |
| `FOLDERBLOG_STORAGE_SECRET` | The secret that goes with `FOLDERBLOG_STORAGE_KEY_ID` |

To make the background helper watch a different folder, type
`systemctl --user edit folderblog` and add these two lines:

```
[Service]
Environment=FOLDERBLOG_ROOT=/path/to/blogs
```

The storage key variables go there the same way
(`Environment=FOLDERBLOG_STORAGE_KEY_ID=...`), if the helper should use them.

Commands in `[build]` and `type = "command"` publishing also get `FOLDERBLOG_OUT` (the
finished website folder) and `FOLDERBLOG_BLOG` (the blog's folder).

## When something goes wrong

**Nothing happens when I save.** Type `folderblog status`. If the helper isn't running,
type `folderblog install-service`.

**`folderblog install-service` fails with "systemctl --user daemon-reload failed".** There
is no systemd user session (a container, a server, or an SSH login). Run `folderblog
watch` in a terminal or in `tmux` instead, or run `loginctl enable-linger $USER`, log in
again and try once more. See "Step 3: Start the helper".

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

- It only **reads** your writing. It never changes your posts or pages. On its own it
  only writes its own files (`.blog/`, `dates.toml`, `AGENTS.md`, and sample files in
  `templates/`).
  Episodes change only when you publish one, or when an AI helper you connected does.
- Each build happens in a fresh place. A broken build never touches your live website.
- It ignores the temporary files that editors and sync programs leave around.
- Each blog builds on its own, so one broken blog never stops the others.
- Your GitHub login stays on your computer. Only you can read it.
- Your original photos never go online, so their hidden location data stays private.
- Your Cloudflare key stays on your computer too, outside the blog folder. It never shows
  up in `blog.toml`, in messages or in what an AI helper sees.
- Designs can't run programs on your computer. Only your own `blog.toml` can.

## Getting more help

- `folderblog --help` lists every command.
- `folderblog COMMAND --help` explains one command.
- `folderblog guide` and `man folderblog` show this guide.
- The README has more technical details: <https://github.com/ninepointlabs/folderblog>
- The website has a video tour: <https://folderblog.ninepointlabs.com>
