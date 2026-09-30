---
tags: [release]
---
# Introducing folderblog

I've started more blogs than I've finished. Every time, the reason I stopped was the same: something stood between writing and publishing. Front matter to remember. A build step. A commit. A deploy that broke because a dependency moved.

folderblog removes all of it. **A folder is a blog.** Write a Markdown file, save it, and a few seconds later it's live.

<!--more-->

## What 0.1.0 does

- A background service watches `~/Blogs`. Every folder in it is its own blog.
- Posts need no front matter. The title comes from the first heading, the address from the file name, and the date from the moment folderblog first saw the post. That date is recorded once and never moves.
- Publishing to GitHub Pages needs no trips to GitHub. You log in once through your own small GitHub App, and after that `folderblog new myblog` creates the repository and turns on Pages by itself.
- Builds happen in a fresh folder and replace the live site only if everything rendered. A broken template changes nothing, and you get a notification naming the file and line.
- Themes are Jinja templates that own every byte of the site. The engine owns only content discovery, Markdown, URLs, the RSS feed and the sitemap.

## Designed by agents

Every blog comes with an `AGENTS.md` file: the whole theme contract, written for a coding agent. To test it, we gave three agents the same blog and a one-line brief each. One built a text-only site, one a dense magazine with search, and one a sideways film strip. None of them needed a change to the engine. You can see all three on the [home page](/#themes).

## This site

This website is a folderblog blog too. Its source is the [`site/` folder](https://github.com/ninepointlabs/folderblog/tree/main/site) in the repository, and GitHub Actions builds it with the released binary on every push.

[Install folderblog](/#install) · [Read the guide](/guide/) · [Source on GitHub](https://github.com/ninepointlabs/folderblog)
