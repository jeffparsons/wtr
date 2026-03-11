use std::process::Command;

/// Infer the version of a direct dependency from the current workspace.
///
/// Returns `None` if `cargo metadata` fails (e.g. not in a Rust project)
/// or the crate is not a direct dependency.
pub fn infer_dep_version(crate_name: &str) -> Option<String> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;

    let resolve = metadata.get("resolve")?;
    let root_id = resolve.get("root")?.as_str()?;
    let nodes = resolve.get("nodes")?.as_array()?;

    // Find the root node.
    let root_node = nodes.iter().find(|n| n.get("id").and_then(|v| v.as_str()) == Some(root_id))?;

    let deps = root_node.get("deps")?.as_array()?;

    // Normalize for comparison: replace hyphens with underscores.
    let query_normalized = crate_name.replace('-', "_");

    for dep in deps {
        let dep_name = dep.get("name")?.as_str()?;
        if dep_name.replace('-', "_") != query_normalized {
            continue;
        }

        // The `pkg` field looks like `registry+...#crate-name@1.2.3`
        // or just `crate-name@1.2.3` for path deps, etc.
        let pkg = dep.get("pkg")?.as_str()?;
        let version = pkg.split('@').last()?;
        if version.is_empty() {
            continue;
        }
        return Some(version.to_string());
    }

    None
}
