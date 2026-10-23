---
# A podcast episode with every detail folderblog knows about.
# To use it: copy this whole folder into podcast/ and name it after the episode,
# like podcast/2026-03-21-first-episode/. Put the recording beside this file
# (one .mp3, or .mp4 for video). Then change what you need and delete the lines
# you don't. The podcast must be turned on in blog.toml ([podcast] enabled = true).
# Lines starting with # are notes.

# The episode title.
title: "First episode"

# The episode's permanent name tag in podcast apps. Write it once and never change it.
# Leave it out at first: `folderblog check` suggests one you can paste in here.
# guid: "paste the one folderblog check gives you"

# The short line apps show under the title.
summary: "What this show is about, and who it is for."

# Season and episode numbers (whole numbers, 1 or more). Both optional.
season: 1
episode: 1

# full (the default), trailer or bonus.
# episode_type: full

# true marks an episode with grown-up language. Leave it out to use the show's setting.
# explicit: false

# Who is on the episode. role is host, guest, producer and so on; links must be https://.
persons:
  - { name: "Ada Host", role: host, href: "https://example.com/ada" }
  - { name: "Sam Guest", role: guest }

# Labels, as a list or "audio, making things".
tags: [interviews]

# When it came out, like 2026-03-21 or "2026-03-21 14:30".
# Leave it out to use the folder name's date, or else the day folderblog first sees it.
# date: 2026-03-21

# true also lists this episode with your blog posts and in the blog's main feed.
# Leave it out to use the show's setting ([podcast] in_main_feed, off by default).
# in_main_feed: true

# Episode art: a square JPEG or PNG beside this file, 1400 to 3000 pixels wide.
# Leave it out to use the show's art.
# image: art.jpg

# Which files to use, when the folder has more than one of a kind.
# Normally folderblog finds them by itself.
# audio: episode.mp3
# transcript: transcript.vtt
# chapters: chapters.json

# Lightning tips for this episode only. Leave it out to use the show's
# [podcast] lightning_address. split is each person's share.
# value:
#   - { name: "Ada Host", address: "ada@example.com", split: 90 }
#   - { name: "Sam Guest", address: "sam@example.com", split: 10 }

# true keeps it to yourself; you still see it in `folderblog preview`.
# Delete this line (or run `folderblog podcast publish`) when the episode is ready.
draft: true
---
Show notes: what we talked about, and links to the things we mentioned.

- [Something we mentioned](https://example.com)
