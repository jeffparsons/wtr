use anyhow::{Result, bail};
use clap::Parser;
use wtr::{fetch, lookup, render};

#[derive(Parser)]
#[command(name = "wtr", about = "Look up Rust crate documentation from docs.rs")]
struct Cli {
    /// Item path, e.g. "jiff::Timestamp", "serde::Serialize", "tokio::spawn"
    query: String,

    /// Show full documentation
    #[arg(short, long)]
    full: bool,

    /// List methods (inherent impl methods)
    #[arg(short, long)]
    methods: bool,

    /// Show trait implementations
    #[arg(short, long)]
    traits: bool,

    /// All of the above
    #[arg(short, long)]
    all: bool,

    /// Disable colors (also respects NO_COLOR env)
    #[arg(long)]
    no_color: bool,

    /// Bypass cache and re-fetch
    #[arg(long)]
    refresh: bool,

    /// Crate version (default: "latest")
    #[arg(long, default_value = "latest")]
    version: String,
}

fn parse_query(query: &str) -> Result<(String, Vec<String>)> {
    let parts: Vec<&str> = query.split("::").collect();
    if parts.is_empty() || parts[0].is_empty() {
        bail!("invalid query: expected format like `crate::Item`");
    }
    let crate_name = parts[0].to_string();
    let path: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
    Ok((crate_name, path))
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    let show_full = cli.full || cli.all;
    let show_methods = cli.methods || cli.all;
    let show_traits = cli.traits || cli.all;

    let (crate_name, path) = parse_query(&cli.query)?;

    let fetched = fetch::fetch_crate(&crate_name, &cli.version, cli.refresh).await?;
    let krate = &fetched.krate;

    let lookup::LookupResult {
        item,
        reexport_source,
    } = lookup::lookup_item(krate, &path)?;

    let mut body = format!("// {crate_name} {}\n\n", fetched.version);

    if let Some(source) = &reexport_source {
        body += &format!("// note: re-exported from {source}\n\n");
    }

    body += &(if show_full {
        render::render_item_full(item, krate)
    } else {
        render::render_item_summary(item, krate)
    });

    if show_methods {
        body.push('\n');
        body.push_str(&render::render_methods(item, krate));
    }

    if show_traits {
        body.push('\n');
        body.push_str(&render::render_trait_impls(item, krate));
    }

    let suggestions = render::render_suggestions(
        &crate_name,
        &path,
        item,
        show_full,
        show_methods,
        show_traits,
    );

    render::print_output(&body, &suggestions, cli.no_color);

    Ok(())
}
