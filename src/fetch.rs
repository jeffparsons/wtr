use anyhow::{Context, Result, bail};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::time::SystemTime;

const SUPPORTED_FORMAT: u32 = rustdoc_types::FORMAT_VERSION;

/// Resolved crate data: the parsed rustdoc JSON plus the resolved version string.
pub struct FetchedCrate {
    pub krate: rustdoc_types::Crate,
    pub version: String,
}

fn cache_dir() -> Result<PathBuf> {
    let base = dirs::cache_dir().context("could not determine cache directory")?;
    Ok(base.join("wtr"))
}

fn crate_cache_dir(crate_name: &str) -> Result<PathBuf> {
    Ok(cache_dir()?.join(crate_name))
}

/// Check if the "latest" version sidecar is still fresh (< 24h old).
fn read_latest_sidecar(crate_name: &str) -> Result<Option<String>> {
    let sidecar = crate_cache_dir(crate_name)?.join("latest.version");
    if !sidecar.exists() {
        return Ok(None);
    }
    let metadata = fs::metadata(&sidecar)?;
    let age = SystemTime::now().duration_since(metadata.modified()?)?;
    if age.as_secs() > 24 * 60 * 60 {
        return Ok(None);
    }
    let contents = fs::read_to_string(&sidecar)?;
    let version = contents.trim().to_string();
    if version.is_empty() {
        return Ok(None);
    }
    Ok(Some(version))
}

fn write_latest_sidecar(crate_name: &str, version: &str) -> Result<()> {
    let dir = crate_cache_dir(crate_name)?;
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("latest.version"), version)?;
    Ok(())
}

fn cached_json_path(crate_name: &str, version: &str) -> Result<PathBuf> {
    Ok(crate_cache_dir(crate_name)?.join(format!("{version}.json")))
}

fn load_cached(crate_name: &str, version: &str) -> Result<Option<rustdoc_types::Crate>> {
    let path = cached_json_path(crate_name, version)?;
    if !path.exists() {
        return Ok(None);
    }
    let file = fs::File::open(&path)?;
    let reader = std::io::BufReader::new(file);
    let krate: rustdoc_types::Crate = serde_json::from_reader(reader)
        .context("failed to parse cached rustdoc JSON")?;
    Ok(Some(krate))
}

fn save_to_cache(crate_name: &str, version: &str, json_bytes: &[u8]) -> Result<()> {
    let dir = crate_cache_dir(crate_name)?;
    fs::create_dir_all(&dir)?;
    let path = cached_json_path(crate_name, version)?;
    fs::write(&path, json_bytes)?;
    Ok(())
}

/// Fetch rustdoc JSON for a crate from docs.rs, with disk caching.
pub async fn fetch_crate(
    crate_name: &str,
    version: &str,
    refresh: bool,
) -> Result<FetchedCrate> {
    let is_latest = version == "latest";

    // For "latest", try to resolve from sidecar cache first.
    if is_latest && !refresh {
        if let Some(resolved) = read_latest_sidecar(crate_name)? {
            if let Some(krate) = load_cached(crate_name, &resolved)? {
                return Ok(FetchedCrate {
                    krate,
                    version: resolved,
                });
            }
        }
    }

    // For explicit versions, try cache directly.
    if !is_latest && !refresh {
        if let Some(krate) = load_cached(crate_name, version)? {
            return Ok(FetchedCrate {
                krate,
                version: version.to_string(),
            });
        }
    }

    // Fetch from docs.rs.
    let url = format!("https://docs.rs/crate/{crate_name}/{version}/json");
    let response = reqwest::get(&url)
        .await
        .context("failed to connect to docs.rs")?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        bail!(
            "crate `{crate_name}` version `{version}` not found on docs.rs.\n\
             This could mean:\n  \
             - The crate name is misspelled\n  \
             - The version doesn't exist\n  \
             - Rustdoc JSON is not available (only crates published after 2025-05-23 have it)"
        );
    }

    if !response.status().is_success() {
        bail!(
            "docs.rs returned HTTP {}: {}",
            response.status(),
            response.status().canonical_reason().unwrap_or("unknown")
        );
    }

    let compressed = response
        .bytes()
        .await
        .context("failed to read response body")?;

    // Decompress zstd.
    let mut decoder = zstd::Decoder::new(compressed.as_ref())
        .context("failed to initialize zstd decoder")?;
    let mut json_bytes = Vec::new();
    decoder
        .read_to_end(&mut json_bytes)
        .context("failed to decompress zstd data")?;

    let krate = parse_rustdoc_json(&json_bytes, crate_name)?;

    // Resolve the actual version from the parsed crate data.
    let resolved_version = krate
        .crate_version
        .clone()
        .unwrap_or_else(|| version.to_string());

    // Cache the decompressed JSON.
    if let Err(e) = save_to_cache(crate_name, &resolved_version, &json_bytes) {
        eprintln!("warning: failed to cache: {e}");
    }

    // Update the latest sidecar.
    if is_latest {
        if let Err(e) = write_latest_sidecar(crate_name, &resolved_version) {
            eprintln!("warning: failed to write latest sidecar: {e}");
        }
    }

    Ok(FetchedCrate {
        krate,
        version: resolved_version,
    })
}

/// Parse rustdoc JSON from bytes, checking format version first.
pub fn parse_rustdoc_json(json_bytes: &[u8], crate_name: &str) -> Result<rustdoc_types::Crate> {
    check_format_version(json_bytes, crate_name)?;
    serde_json::from_slice(json_bytes).context("failed to parse rustdoc JSON")
}

/// Extract `format_version` from the JSON without full deserialization
/// and check it matches what we support.
fn check_format_version(json_bytes: &[u8], crate_name: &str) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct Partial {
        format_version: u32,
    }
    let partial: Partial = serde_json::from_slice(json_bytes)
        .context("failed to read format_version from rustdoc JSON")?;
    if partial.format_version != SUPPORTED_FORMAT {
        bail!(
            "rustdoc JSON for `{crate_name}` uses format version {}, \
             but wtr supports version {SUPPORTED_FORMAT}.\n\
             The crate may need to be republished to get updated docs, \
             or wtr needs to be updated.",
            partial.format_version
        );
    }
    Ok(())
}
