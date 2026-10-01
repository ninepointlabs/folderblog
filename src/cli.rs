use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "folderblog", version, about = "A folder is a blog: save Markdown, it goes live.")]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Create a new blog folder with the default theme and AGENTS.md (published to GitHub Pages if logged in)
    New {
        /// Blog name (usually its domain), created under the root; or a path
        name: String,
        #[arg(long, env = "FOLDERBLOG_ROOT")]
        root: Option<PathBuf>,
        /// Serve the GitHub Pages site at this custom domain (you add the DNS records)
        #[arg(long)]
        domain: Option<String>,
        /// GitHub repo name (default: the blog name)
        #[arg(long)]
        repo: Option<String>,
        /// Don't set up GitHub Pages even if logged in
        #[arg(long)]
        no_github: bool,
    },
    /// Log in to GitHub so blogs can be published to GitHub Pages with no manual setup
    Login {
        /// Only `github` is supported
        #[arg(default_value = "github")]
        service: String,
        /// Client ID of your GitHub App (only needed the first time; it is remembered)
        #[arg(long)]
        client_id: Option<String>,
    },
    /// Forget the stored GitHub login
    Logout {
        #[arg(default_value = "github")]
        service: String,
    },
    /// Build the blog into .blog/public (or --out) without deploying
    Build {
        blog: Option<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
        /// Include drafts
        #[arg(long)]
        drafts: bool,
        /// Build against a fixture content set instead of the blog's content
        #[arg(long)]
        fixtures: Option<String>,
    },
    /// Serve a live-reloading local preview, drafts included
    Preview {
        blog: Option<PathBuf>,
        #[arg(long, default_value_t = 4000)]
        port: u16,
        /// Render the theme against fixture content (stress, empty, one)
        #[arg(long, num_args = 0..=1, default_missing_value = "stress")]
        fixtures: Option<String>,
    },
    /// Render everything and report template errors, feed problems, broken links, collisions
    Check {
        blog: Option<PathBuf>,
        #[arg(long, num_args = 0..=1, default_missing_value = "stress")]
        fixtures: Option<String>,
    },
    /// Print the exact template context for a URL as JSON (no URL: list all outputs)
    Data {
        url: Option<String>,
        #[arg(long)]
        blog: Option<PathBuf>,
        #[arg(long)]
        drafts: bool,
        #[arg(long)]
        fixtures: Option<String>,
    },
    /// Watch every blog under the root; rebuild and deploy on change
    Watch {
        #[arg(long, env = "FOLDERBLOG_ROOT")]
        root: Option<PathBuf>,
    },
    /// Build and deploy a blog now
    Deploy {
        blog: Option<PathBuf>,
        /// Deploy even if the output is unchanged since the last deploy
        #[arg(long)]
        force: bool,
    },
    /// Show build/deploy status for every blog under the root
    Status {
        #[arg(long, env = "FOLDERBLOG_ROOT")]
        root: Option<PathBuf>,
        /// Machine-readable output
        #[arg(long)]
        json: bool,
    },
    /// Install (and start) the systemd user service that runs `watch`
    InstallService {
        #[arg(long, env = "FOLDERBLOG_ROOT")]
        root: Option<PathBuf>,
        /// Only write the unit file; do not enable or start it
        #[arg(long)]
        no_start: bool,
        /// Print the unit file instead of installing it
        #[arg(long)]
        print: bool,
    },
    /// Turn the photo gallery on or off; on adds gallery/ and the gallery templates (no argument: show its state)
    Gallery {
        #[arg(value_parser = ["on", "off"])]
        state: Option<String>,
        blog: Option<PathBuf>,
    },
    /// Regenerate the contract section of the blog's AGENTS.md (prints it without --write)
    AgentsMd {
        blog: Option<PathBuf>,
        #[arg(long)]
        write: bool,
    },
}

/// (name, about) for every subcommand, as documented in AGENTS.md.
pub fn command_docs() -> Vec<(String, String)> {
    Cli::command()
        .get_subcommands()
        .filter(|c| c.get_name() != "help")
        .map(|c| {
            let mut args: Vec<String> = c
                .get_positionals()
                .map(|a| format!("[{}]", a.get_id().as_str().to_uppercase()))
                .collect();
            for a in c.get_opts() {
                if let Some(l) = a.get_long() {
                    let takes = a.get_num_args().is_some_and(|n| n.max_values() > 0);
                    let v = a.get_value_names().and_then(|v| v.first()).map(|v| v.to_string()).unwrap_or_else(|| a.get_id().as_str().to_uppercase());
                    args.push(if takes { format!("[--{l} {v}]") } else { format!("[--{l}]") });
                }
            }
            let name = if args.is_empty() { c.get_name().to_string() } else { format!("{} {}", c.get_name(), args.join(" ")) };
            (name, c.get_about().map(|a| a.to_string()).unwrap_or_default())
        })
        .collect()
}
