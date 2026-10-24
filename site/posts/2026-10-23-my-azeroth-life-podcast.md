---
tags: [example]
summary: "A real podcast made with folderblog: My Azeroth Life turns World of Warcraft play sessions into blog posts and narrated episodes, all from one blog folder."
---
# A real podcast on folderblog: My Azeroth Life

folderblog's podcast feature now runs a real show. [My Azeroth Life](https://myazerothlife.com/) is a blog about my World of Warcraft characters, and [its podcast](https://myazerothlife.com/podcast/) turns those stories into narrated episodes. Both come out of one folder in `~/Blogs`. Here is how it is put together, in case it helps you set up your own.

<!--more-->

## Where the stories come from

A WoW addon I wrote, StoryTracker, takes notes while I play: quests, zones, levels, deaths, loot, dungeon bosses, gold. A small script pulls out what's new for each character, and every morning an AI agent writes a post from it in the voice of a weathered chronicler. The post is saved into the blog's `drafts/` folder, which only shows up in `folderblog preview`. I read it, and if it's right I move it into `posts/`. A few seconds later it's live. AI does the writing, and nothing goes up until I've read it.

A post is just a Markdown file with one line of tags at the top. The title comes from the first heading and the date from the filename:

```markdown
---
tags: [wow, storytracker, Retail, laotou, monk]
---

# The Scarab at Runestone Falithas

The scarab was fat with something that wasn't its own...
```

## Making the episodes

Claude Code on my workstation turns a batch of posts into a script, then voices it with ElevenLabs: a narrator designed for each character (an old lorewalker for the Pandaren monk, a Kul Tiran sea captain for the warrior), plus music and sound effects. ffmpeg mixes it, keeps the music under the voice, brings it to podcast loudness and embeds the chapters. Every line is run back through speech-to-text and compared with the script, and then I listen to the whole thing. None of that is part of folderblog. What folderblog gets at the end is an MP3 and a chapters file.

## The podcast folder

`folderblog podcast on` added this to the blog:

```
podcast/
  index.md                              the show's title and description
  cover.jpg                             show art
  2026-10-23-the-chronicle-of-laotou/
    index.md                            show notes and front matter
    episode.mp3
    chapters.json
  2026-10-23-the-chronicle-of-burrly/
    index.md
    episode.mp3
    chapters.json
```

An episode's `index.md` holds the show notes and a little front matter: the title, a summary, the season and episode number, the host, tags and a guid that never changes. I never typed the length, file size or duration. folderblog reads them from the MP3. Each episode starts with `draft: true`, so it stays on my computer, and is never uploaded, until I've listened to it.

## What folderblog builds from it

- **The show page**, [/podcast/](https://myazerothlife.com/podcast/), with a Subscribe box near the top that shows the feed address.
- **A page for each episode**, like [The Chronicle of Burrly](https://myazerothlife.com/podcast/the-chronicle-of-burrly/), with a player and a chapter list you can click to jump around.
- **A separate feed**, [/podcast/feed.xml](https://myazerothlife.com/podcast/feed.xml), with the iTunes tags plus Podcasting 2.0 ones: `podcast:guid`, `podcast:locked`, chapters, the people on each episode, and `podcast:value` with a Lightning address for tips. The blog's own feed stays posts-only, because `in_main_feed` is off.

The site uses a custom parchment theme, and the podcast pages were restyled to match through `theme/_podcast/`. The theme is just templates, and `AGENTS.md` explained them well enough that Claude did most of that work.

## Audio on R2

The MP3s are 30 MB or so each, and none of that goes into git or the GitHub Pages site. `folderblog podcast setup` connected a Cloudflare R2 bucket once, and the key lives in `~/.config/folderblog/`, not in the blog folder. When an episode is published, folderblog uploads the audio, checks that the hosted file is really there and the right size, and only then pushes the site and feed. If an upload fails, nothing goes out and it tries again later.

Before publishing, `folderblog check` goes over the episodes: required tags, https links, a known length and duration, real MP3 data, cover art size, the category, valid chapters, and guids that haven't changed since the last deploy.

## Going weekly

From here on there will be up to two episodes a week in the same feed: a Retail show on Sunday nights and a WoW Forever show on Monday nights, starting after Forever launches on November 4. Each is just another folder in `podcast/`, tagged with its game.

I wrote up the whole pipeline, addon included, on the [Nine Point Labs blog](https://blog.ninepointlabs.com/2026/10/how-my-azeroth-life-is-made/). If you want to try it with your own recordings, `folderblog podcast on` is the place to start, and the [podcast section of the guide](https://github.com/ninepointlabs/folderblog/blob/main/docs/GUIDE.md#start-a-podcast) walks through the rest.
