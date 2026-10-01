//! The user guide (`docs/GUIDE.md`), shown by `folderblog guide`, and the same text as a
//! roff manual page for `man folderblog`. One source, so the two never disagree.

use std::io::{IsTerminal, Write};
use std::process::{Command, Stdio};

pub const GUIDE: &str = include_str!("../docs/GUIDE.md");

/// Write `text` to stdout, through `program args` when stdout is a terminal and the
/// program exists (a pager, or `man`), so a long text doesn't scroll past.
pub fn show(text: &str, program: &str, args: &[&str]) -> anyhow::Result<()> {
    if std::io::stdout().is_terminal() {
        if let Ok(mut child) = Command::new(program).args(args).stdin(Stdio::piped()).spawn() {
            if let Some(mut stdin) = child.stdin.take() {
                // The reader may quit before reading everything; that's fine.
                let _ = stdin.write_all(text.as_bytes());
            }
            child.wait()?;
            return Ok(());
        }
    }
    let mut out = std::io::stdout().lock();
    // Piping into `head` closes stdout early; that's not an error.
    let _ = out.write_all(text.as_bytes());
    Ok(())
}

/// Escape text for roff and turn Markdown inline marks into font changes.
fn inline(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut bold = false;
    let mut italic = false;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '`' => {
                let end = chars[i + 1..].iter().position(|&x| x == '`').map(|p| i + 1 + p);
                if let Some(end) = end {
                    let code: String = chars[i + 1..end].iter().collect();
                    out.push_str(&format!("\\fB{}\\fR", escape(&code, true)));
                    i = end + 1;
                    continue;
                }
                out.push('`');
            }
            '*' if chars.get(i + 1) == Some(&'*') => {
                bold = !bold;
                out.push_str(if bold { "\\fB" } else { "\\fR" });
                i += 2;
                continue;
            }
            '*' if italic || chars.get(i + 1).is_some_and(|n| !n.is_whitespace()) => {
                italic = !italic;
                out.push_str(if italic { "\\fI" } else { "\\fR" });
            }
            '[' => {
                // [text](url)
                let close = chars[i..].iter().position(|&x| x == ']').map(|p| i + p);
                if let Some(close) = close {
                    if chars.get(close + 1) == Some(&'(') {
                        if let Some(end) = chars[close..].iter().position(|&x| x == ')').map(|p| close + p) {
                            let text: String = chars[i + 1..close].iter().collect();
                            let url: String = chars[close + 2..end].iter().collect();
                            out.push_str(&inline(&text));
                            if text != url {
                                out.push_str(&format!(" <{}>", escape(&url, false)));
                            }
                            i = end + 1;
                            continue;
                        }
                    }
                }
                out.push('[');
            }
            '<' if s[s.char_indices().nth(i).unwrap().0..].starts_with("<http") => {
                let end = chars[i..].iter().position(|&x| x == '>').map(|p| i + p).unwrap_or(chars.len() - 1);
                let url: String = chars[i + 1..end].iter().collect();
                out.push_str(&escape(&url, false));
                i = end + 1;
                continue;
            }
            _ => out.push_str(&escape(&c.to_string(), false)),
        }
        i += 1;
    }
    out
}

fn escape(s: &str, code: bool) -> String {
    let s = s.replace('\\', "\\e");
    if code { s.replace('-', "\\-") } else { s }
}

/// Lines must not start with `.` or `'`, which roff reads as requests.
fn protect(line: &str) -> String {
    if line.starts_with('.') || line.starts_with('\'') { format!("\\&{line}") } else { line.to_string() }
}

pub fn render() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let mut out = format!(
        ".TH FOLDERBLOG 1 \"\" \"folderblog {version}\" \"folderblog manual\"\n\
         .SH NAME\nfolderblog \\- a folder is a blog: save Markdown, it goes live\n\
         .SH SYNOPSIS\n.B folderblog\n\\fICOMMAND\\fR [\\fIOPTIONS\\fR]\n.PP\n\
         .B folderblog guide\n.br\n.B folderblog \\-\\-help\n.SH DESCRIPTION\n"
    );
    let lines: Vec<&str> = GUIDE.lines().collect();
    let mut i = 0;
    let mut para: Vec<String> = vec![];
    let flush = |para: &mut Vec<String>, out: &mut String| {
        if !para.is_empty() {
            out.push_str(".PP\n");
            out.push_str(&protect(&inline(&para.join(" "))));
            out.push('\n');
            para.clear();
        }
    };
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim_end();
        if t.starts_with("```") {
            flush(&mut para, &mut out);
            out.push_str(".PP\n.RS 4\n.nf\n");
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                out.push_str(&protect(&escape(lines[i], true)));
                out.push('\n');
                i += 1;
            }
            out.push_str(".fi\n.RE\n");
        } else if let Some(h) = t.strip_prefix("# ") {
            // The document title is the man page's NAME.
            let _ = h;
            flush(&mut para, &mut out);
        } else if let Some(h) = t.strip_prefix("## ") {
            flush(&mut para, &mut out);
            out.push_str(&format!(".SH {}\n", protect(&inline(&h.to_uppercase()))));
        } else if let Some(h) = t.strip_prefix("### ") {
            flush(&mut para, &mut out);
            out.push_str(&format!(".SS {}\n", protect(&inline(h))));
        } else if t.starts_with('|') {
            flush(&mut para, &mut out);
            // A table: header row, separator, rows. Each row becomes a tagged paragraph.
            let mut rows = vec![];
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                let cells: Vec<String> =
                    lines[i].trim().trim_matches('|').split('|').map(|c| c.trim().to_string()).collect();
                if !cells.iter().all(|c| c.chars().all(|ch| ch == '-' || ch == ':')) {
                    rows.push(cells);
                }
                i += 1;
            }
            for row in rows.iter().skip(1) {
                out.push_str(&format!(".TP\n{}\n{}\n", protect(&inline(&row[0])), protect(&inline(&row[1..].join(" ")))));
            }
            continue;
        } else if let Some(item) = t.strip_prefix("- ") {
            flush(&mut para, &mut out);
            let mut text = vec![item.to_string()];
            while i + 1 < lines.len() && lines[i + 1].starts_with("  ") && !lines[i + 1].trim().is_empty() {
                i += 1;
                text.push(lines[i].trim().to_string());
            }
            out.push_str(&format!(".IP \\(bu 2\n{}\n", protect(&inline(&text.join(" ")))));
        } else if let Some((n, item)) = t.split_once(". ").filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) {
            flush(&mut para, &mut out);
            let mut text = vec![item.to_string()];
            while i + 1 < lines.len() && lines[i + 1].starts_with("   ") && !lines[i + 1].trim().is_empty() {
                i += 1;
                text.push(lines[i].trim().to_string());
            }
            out.push_str(&format!(".IP {n}. 4\n{}\n", protect(&inline(&text.join(" ")))));
        } else if let Some(q) = t.strip_prefix("> ") {
            flush(&mut para, &mut out);
            let mut text = vec![q.to_string()];
            while i + 1 < lines.len() && lines[i + 1].starts_with('>') {
                i += 1;
                text.push(lines[i].trim_start_matches('>').trim().to_string());
            }
            out.push_str(&format!(".PP\n.RS 4\n{}\n.RE\n", protect(&inline(&text.join(" ")))));
        } else if t.is_empty() {
            flush(&mut para, &mut out);
        } else {
            para.push(t.trim().to_string());
        }
        i += 1;
    }
    flush(&mut para, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_marks_become_fonts() {
        assert_eq!(inline("a `--x` b"), "a \\fB\\-\\-x\\fR b");
        assert_eq!(inline("**bold** and *it*"), "\\fBbold\\fR and \\fIit\\fR");
        assert_eq!(inline("[site](https://a.b)"), "site <https://a.b>");
        assert_eq!(inline("<https://a.b>"), "https://a.b");
        assert_eq!(inline("2 * 3"), "2 * 3");
    }

    #[test]
    fn guide_documents_every_command_and_option() {
        use clap::CommandFactory;
        let cli = crate::cli::Cli::command();
        for c in cli.get_subcommands() {
            let name = c.get_name();
            assert!(GUIDE.contains(&format!("### folderblog {name}")), "guide has no section for `folderblog {name}`");
            for a in c.get_arguments() {
                if let Some(l) = a.get_long().filter(|l| *l != "help") {
                    assert!(GUIDE.contains(&format!("--{l}")), "guide never mentions `folderblog {name} --{l}`");
                }
            }
        }
    }

    #[test]
    fn man_page_renders_cleanly() {
        let roff = render();
        assert!(roff.starts_with(".TH FOLDERBLOG 1"));
        assert!(roff.contains(".SH EVERY COMMAND, IN DETAIL"));
        for line in roff.lines() {
            assert!(!line.contains("```") && !line.starts_with("|"), "markdown leaked: {line}");
        }
        // With groff installed, it must render without warnings.
        let Ok(mut child) = Command::new("groff")
            .args(["-man", "-ww", "-z", "-Tutf8"])
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        else {
            return;
        };
        child.stdin.take().unwrap().write_all(roff.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success() && stderr.trim().is_empty(), "groff: {stderr}");
    }
}
