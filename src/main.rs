use anyhow::{Result, bail};
use clap::Parser;
use wtr::fetch::VersionSource;
use wtr::{fetch, lookup, render, workspace};

#[derive(Parser)]
#[command(name = "wtr", about = "Look up Rust crate documentation from docs.rs")]
struct Cli {
    /// Item path, e.g. "jiff::Timestamp", "serde::Serialize", "tokio::spawn"
    query: String,

    /// Search for items by name within the crate (e.g. "wtr bevy Material")
    search: Option<String>,

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

/// Resolve a version string for a crate. For cross-crate fetches, always
/// use workspace inference or "latest" (never the user's explicit `--version`).
fn resolve_version(crate_name: &str) -> (String, VersionSource) {
    match workspace::infer_dep_version(crate_name) {
        Some(v) => (v, VersionSource::Workspace),
        None => ("latest".to_string(), VersionSource::Latest),
    }
}

async fn run(cli: Cli) -> Result<()> {
    let show_full = cli.full || cli.all;
    let show_methods = cli.methods || cli.all;
    let show_traits = cli.traits || cli.all;

    let (crate_name, path) = parse_query(&cli.query)?;

    let (effective_version, version_source) = if cli.version == "latest" {
        resolve_version(&crate_name)
    } else {
        (cli.version.clone(), VersionSource::Explicit)
    };

    let is_workspace = matches!(version_source, VersionSource::Workspace);

    let (fetched, latest_version) = if is_workspace {
        let (fetched, latest) = tokio::join!(
            fetch::fetch_crate(&crate_name, &effective_version, cli.refresh, version_source),
            fetch::check_latest_version(&crate_name),
        );
        (fetched?, latest)
    } else {
        let fetched =
            fetch::fetch_crate(&crate_name, &effective_version, cli.refresh, version_source)
                .await?;
        (fetched, None)
    };

    let krate = &fetched.krate;

    let header = if is_workspace {
        let annotation = match latest_version {
            Some(ref latest) if *latest == fetched.version => {
                " (from workspace, latest)".to_string()
            }
            Some(ref latest) => format!(" (from workspace; latest is {latest})"),
            None => " (from workspace)".to_string(),
        };
        format!("// {crate_name} {}{annotation}\n\n", fetched.version)
    } else {
        format!("// {crate_name} {}\n\n", fetched.version)
    };

    // Search mode: find items by name within the crate.
    if let Some(ref search_term) = cli.search {
        let results = lookup::search_items(krate, search_term);
        if !results.is_empty() {
            let mut body = header;
            body += &format!(
                "Search results for \"{}\" ({} found):\n\n",
                search_term,
                results.len()
            );
            body += &render::render_search_results(&results, &crate_name);
            render::print_output(&body, "", cli.no_color);
            return Ok(());
        }

        // Fall back to cross-crate search.
        let external = lookup::search_external_items(krate, search_term);
        if external.is_empty() {
            let mut body = header;
            body += &format!("No items matching \"{search_term}\" found.\n");
            render::print_output(&body, "", cli.no_color);
            return Ok(());
        }

        // Group by source crate and search each one.
        let mut source_crates: Vec<String> = external.iter().map(|e| e.crate_name.clone()).collect();
        source_crates.sort();
        source_crates.dedup();

        let mut body = header;
        let mut total_found = 0;
        for source_crate in &source_crates {
            let (src_version, src_version_source) = resolve_version(source_crate);
            let src_fetched = match fetch::fetch_crate(
                source_crate,
                &src_version,
                cli.refresh,
                src_version_source,
            )
            .await
            {
                Ok(f) => f,
                Err(_) => continue,
            };
            let src_results = lookup::search_items(&src_fetched.krate, search_term);
            if !src_results.is_empty() {
                total_found += src_results.len();
                body += &format!(
                    "// {} {} → {} {}\n",
                    crate_name, fetched.version, source_crate, src_fetched.version
                );
                body += &format!(
                    "Search results for \"{}\" ({} found):\n\n",
                    search_term,
                    src_results.len()
                );
                body += &render::render_search_results(&src_results, source_crate);
                body += "\n";
            }
        }

        if total_found == 0 {
            body += &format!("No items matching \"{search_term}\" found.\n");
        }
        render::print_output(&body, "", cli.no_color);
        return Ok(());
    }

    // Normal lookup mode.
    //
    // If the item isn't found locally, try cross-crate resolution for
    // re-exports from subcrates (e.g. bevy re-exporting bevy_mesh::Mesh).
    if lookup::lookup_item(krate, &path).is_err()
        && let Some(ext) = lookup::find_external_item(krate, &path)
    {
        let (src_version, src_version_source) = resolve_version(&ext.crate_name);
        if let Ok(src_fetched) = fetch::fetch_crate(
            &ext.crate_name,
            &src_version,
            cli.refresh,
            src_version_source,
        )
        .await
            && let Ok(result) = lookup::lookup_item(&src_fetched.krate, &ext.path)
        {
            let mut body = format!(
                "// {} {} \u{2192} {} {}\n",
                crate_name, fetched.version, ext.crate_name, src_fetched.version
            );
            body += &format!(
                "// note: re-exported from {}::{}\n\n",
                ext.crate_name,
                ext.path.join("::")
            );

            let item = result.item;
            let src_krate = &src_fetched.krate;

            body += &(if show_full {
                render::render_item_full(item, src_krate)
            } else {
                render::render_item_summary(item, src_krate)
            });

            if show_methods {
                body.push('\n');
                body.push_str(&render::render_methods(item, src_krate));
            }

            if show_traits {
                body.push('\n');
                body.push_str(&render::render_trait_impls(item, src_krate));
            }

            let suggestions = render::render_suggestions(
                &ext.crate_name,
                &ext.path,
                item,
                src_krate,
                show_full,
                show_methods,
                show_traits,
            );

            render::print_output(&body, &suggestions, cli.no_color);
            return Ok(());
        }
    }

    let lookup::LookupResult {
        item,
        reexport_source,
    } = match lookup::lookup_item(krate, &path) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Error: {e:#}");
            if let Some(last) = path.last() {
                let results = lookup::search_items(krate, last);
                if !results.is_empty() {
                    let truncated: Vec<_> = results.into_iter().take(5).collect();
                    eprintln!(
                        "\nDid you mean?\n\n{}",
                        render::render_search_results(&truncated, &crate_name)
                    );
                }
            }
            std::process::exit(1);
        }
    };

    let mut body = header;

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
        krate,
        show_full,
        show_methods,
        show_traits,
    );

    render::print_output(&body, &suggestions, cli.no_color);

    Ok(())
}
