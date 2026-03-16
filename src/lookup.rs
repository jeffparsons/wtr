use anyhow::{Result, bail};
use rustdoc_types::{Id, Item, ItemEnum, ItemKind};

use crate::fetch::Crate;

/// Result of looking up an item, with optional re-export metadata.
pub struct LookupResult<'a> {
    pub item: &'a Item,
    /// If the item was found via a re-export, the source path from the `use` item.
    pub reexport_source: Option<String>,
}

/// Find an item by path components (e.g. `["Timestamp"]` or `["de", "Deserialize"]`).
///
/// For method lookups like `["Timestamp", "now"]`, first finds the type,
/// then searches its impl blocks for the method.
pub fn lookup_item<'a>(krate: &'a Crate, path: &[String]) -> Result<LookupResult<'a>> {
    if path.is_empty() {
        // Return the root module.
        let item = krate
            .index
            .get(&krate.root)
            .ok_or_else(|| anyhow::anyhow!("root item not found in index"))?;
        return Ok(LookupResult {
            item,
            reexport_source: None,
        });
    }

    // Try direct path match first (works for top-level items and nested module paths).
    if let Some(item) = find_by_path(krate, path) {
        return Ok(LookupResult {
            item,
            reexport_source: None,
        });
    }

    // Try module-tree walk (finds re-exported items).
    if let Some(result) = find_by_module_walk(krate, path) {
        return Ok(result);
    }

    // Try as a method/associated item: treat last component as method name,
    // rest as the type path.
    if path.len() >= 2 {
        let (type_path, method_name) = path.split_at(path.len() - 1);
        let method_name = &method_name[0];

        // Try find_by_path for the type first.
        let type_result = find_by_path(krate, type_path)
            .map(|item| LookupResult {
                item,
                reexport_source: None,
            })
            .or_else(|| find_by_module_walk(krate, type_path));

        if let Some(type_result) = type_result
            && let Some(method) = find_assoc_item(krate, type_result.item, method_name)
        {
            return Ok(LookupResult {
                item: method,
                reexport_source: type_result.reexport_source,
            });
        }
    }

    let full_path = path.join("::");
    bail!("item `{full_path}` not found")
}

/// Search `krate.paths` for an entry whose path suffix matches the query,
/// then look up the full item in `krate.index`.
fn find_by_path<'a>(krate: &'a Crate, query: &[String]) -> Option<&'a Item> {
    for (id, summary) in &krate.paths {
        // The summary path includes the crate name as the first element,
        // so we check if the tail matches our query.
        if summary.path.len() > query.len() {
            let tail = &summary.path[summary.path.len() - query.len()..];
            if tail == query
                && let Some(item) = krate.index.get(id)
            {
                return Some(item);
            }
        }
    }
    None
}

/// Walk the module tree to find an item by matching query components left-to-right.
///
/// This handles re-exported items that aren't in `krate.paths` under the
/// public path (e.g., `RangeMap` re-exported from `map::RangeMap`).
pub fn find_by_module_walk<'a>(krate: &'a Crate, query: &[String]) -> Option<LookupResult<'a>> {
    let root_item = krate.index.get(&krate.root)?;
    let ItemEnum::Module(ref root_module) = root_item.inner else {
        return None;
    };

    walk_module_children(krate, &root_module.items, query)
}

/// Recursively walk module children matching query components.
fn walk_module_children<'a>(
    krate: &'a Crate,
    children: &[Id],
    query: &[String],
) -> Option<LookupResult<'a>> {
    if query.is_empty() {
        return None;
    }

    let target_name = &query[0];
    let remaining = &query[1..];

    for child_id in children {
        let Some(child_item) = krate.index.get(child_id) else {
            continue;
        };

        match &child_item.inner {
            ItemEnum::Use(use_data) => {
                if &use_data.name == target_name {
                    if remaining.is_empty() {
                        // Final component matched a re-export; follow to target.
                        let target_item = use_data
                            .id
                            .as_ref()
                            .and_then(|id| krate.index.get(id))?;
                        return Some(LookupResult {
                            item: target_item,
                            reexport_source: Some(use_data.source.clone()),
                        });
                    }
                    // Intermediate component matched a re-export pointing to a module.
                    if let Some(id) = &use_data.id
                        && let Some(target_item) = krate.index.get(id)
                        && let ItemEnum::Module(ref module) = target_item.inner
                    {
                        return walk_module_children(krate, &module.items, remaining);
                    }
                }
            }
            _ => {
                if child_item.name.as_deref() == Some(target_name) {
                    if remaining.is_empty() {
                        return Some(LookupResult {
                            item: child_item,
                            reexport_source: None,
                        });
                    }
                    // If it's a module, recurse into it.
                    if let ItemEnum::Module(ref module) = child_item.inner {
                        return walk_module_children(krate, &module.items, remaining);
                    }
                }
            }
        }
    }

    None
}

/// Find a method or associated item on a type by searching its impl blocks.
fn find_assoc_item<'a>(krate: &'a Crate, type_item: &Item, name: &str) -> Option<&'a Item> {
    let impl_ids = get_impl_ids(type_item)?;
    for impl_id in impl_ids {
        let impl_item = krate.index.get(impl_id)?;
        if let ItemEnum::Impl(ref impl_data) = impl_item.inner {
            for item_id in &impl_data.items {
                if let Some(item) = krate.index.get(item_id)
                    && item.name.as_deref() == Some(name)
                {
                    return Some(item);
                }
            }
        }
    }
    None
}

/// Get the list of impl block IDs from a type item.
fn get_impl_ids(item: &Item) -> Option<&Vec<Id>> {
    match &item.inner {
        ItemEnum::Struct(s) => Some(&s.impls),
        ItemEnum::Enum(e) => Some(&e.impls),
        ItemEnum::Union(u) => Some(&u.impls),
        _ => None,
    }
}

/// Collect inherent methods (non-trait impl items) for a type.
pub fn find_methods<'a>(krate: &'a Crate, type_item: &Item) -> Vec<&'a Item> {
    let Some(impl_ids) = get_impl_ids(type_item) else {
        return Vec::new();
    };
    let mut methods = Vec::new();
    for impl_id in impl_ids {
        let Some(impl_item) = krate.index.get(impl_id) else {
            continue;
        };
        let ItemEnum::Impl(ref impl_data) = impl_item.inner else {
            continue;
        };
        // Only inherent impls (no trait).
        if impl_data.trait_.is_some() {
            continue;
        }
        for item_id in &impl_data.items {
            if let Some(item) = krate.index.get(item_id) {
                methods.push(item);
            }
        }
    }
    methods
}

// ── Search ──────────────────────────────────────────────────────────────

pub struct SearchResult<'a> {
    pub item: &'a Item,
    pub path: &'a [String],
    pub kind: ItemKind,
    /// True if the item name matches the search term exactly (case-insensitive).
    pub exact: bool,
}

const MAX_SEARCH_RESULTS: usize = 20;

/// Kinds to exclude from search results (internal details).
fn is_excluded_kind(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Impl | ItemKind::Variant | ItemKind::StructField
    )
}

/// Search for items in a crate whose name matches the given term.
///
/// Uses case-insensitive substring matching, with exact matches flagged.
/// Results are sorted with exact matches first, then alphabetically by path.
pub fn search_items<'a>(krate: &'a Crate, term: &str) -> Vec<SearchResult<'a>> {
    let term_lower = term.to_lowercase();
    let mut results: Vec<SearchResult<'a>> = Vec::new();

    for (id, summary) in &krate.paths {
        if is_excluded_kind(summary.kind) {
            continue;
        }

        // Skip the crate root module (path is just the crate name).
        if summary.path.len() <= 1 {
            continue;
        }

        let Some(item_name) = summary.path.last() else {
            continue;
        };
        let name_lower = item_name.to_lowercase();
        if !name_lower.contains(&term_lower) {
            continue;
        }

        let Some(item) = krate.index.get(id) else {
            continue;
        };

        let exact = name_lower == term_lower;
        results.push(SearchResult {
            item,
            path: &summary.path,
            kind: summary.kind,
            exact,
        });
    }

    // Exact matches first, then alphabetically by path.
    results.sort_by(|a, b| {
        b.exact
            .cmp(&a.exact)
            .then_with(|| a.path.cmp(b.path))
    });

    results.truncate(MAX_SEARCH_RESULTS);
    results
}

/// Collect trait implementations for a type.
/// Returns (impl_item, trait_path_name) pairs.
pub fn find_trait_impls<'a>(krate: &'a Crate, type_item: &Item) -> Vec<(&'a Item, String)> {
    let Some(impl_ids) = get_impl_ids(type_item) else {
        return Vec::new();
    };
    let mut trait_impls = Vec::new();
    for impl_id in impl_ids {
        let Some(impl_item) = krate.index.get(impl_id) else {
            continue;
        };
        let ItemEnum::Impl(ref impl_data) = impl_item.inner else {
            continue;
        };
        if let Some(ref trait_path) = impl_data.trait_ {
            // Skip synthetic/blanket impls for cleaner output.
            if impl_data.is_synthetic || impl_data.blanket_impl.is_some() {
                continue;
            }
            trait_impls.push((impl_item, trait_path.path.clone()));
        }
    }
    trait_impls
}
