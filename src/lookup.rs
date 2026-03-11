use anyhow::{Result, bail};
use rustdoc_types::{Id, Item, ItemEnum};

use crate::fetch::Crate;

/// Find an item by path components (e.g. `["Timestamp"]` or `["de", "Deserialize"]`).
///
/// For method lookups like `["Timestamp", "now"]`, first finds the type,
/// then searches its impl blocks for the method.
pub fn lookup_item<'a>(krate: &'a Crate, path: &[String]) -> Result<&'a Item> {
    if path.is_empty() {
        // Return the root module.
        return krate
            .index
            .get(&krate.root)
            .ok_or_else(|| anyhow::anyhow!("root item not found in index"));
    }

    // Try direct path match first (works for top-level items and nested module paths).
    if let Some(item) = find_by_path(krate, path) {
        return Ok(item);
    }

    // Try as a method/associated item: treat last component as method name,
    // rest as the type path.
    if path.len() >= 2 {
        let (type_path, method_name) = path.split_at(path.len() - 1);
        let method_name = &method_name[0];

        if let Some(type_item) = find_by_path(krate, type_path)
            && let Some(method) = find_assoc_item(krate, type_item, method_name)
        {
            return Ok(method);
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
