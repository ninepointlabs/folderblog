# Kitchen sink: every Markdown feature

An intro paragraph with **bold**, *italic*, ~~strikethrough~~, `inline code`, a [link to another post](2024-10-15-code-heavy.md), an [external link](https://example.org "Example"), and a footnote.[^1] Smart "quotes" -- and dashes --- are typographic.

## Lists

- One
- Two
  - Nested two-a
  - Nested two-b
    1. Deep ordered
    2. Deep ordered again
- Three

1. First
2. Second

- [x] A done task
- [ ] An open task

## A table

| Language | Typing | First appeared | Notes |
|:---------|:------:|---------------:|-------|
| Rust | static | 2010 | Ownership and borrowing |
| Python | dynamic | 1991 | A very long cell that should wrap or scroll rather than blow out the layout on narrow screens |
| Jinja | — | 2008 | Templates |

## Quotes and rules

> A blockquote that goes on for a while so that it wraps onto multiple lines in most layouts, with *emphasis* inside.
>
> > And a nested quote.

---

### Heading level three

#### Heading level four

##### Heading level five

###### Heading level six

## Duplicate heading

## Duplicate heading

## Raw HTML

<figure>
  <img src="images/wide.svg" alt="A wide placeholder image">
  <figcaption>A figure written as raw HTML, with a caption.</figcaption>
</figure>

<details><summary>Raw HTML details element</summary>Hidden content.</details>

A line with a<br>manual break and an image inline: ![small square](images/square.svg).

[^1]: The footnote text, with a [link](https://example.org).
