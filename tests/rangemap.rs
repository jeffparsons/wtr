use std::fs;

use wtr::fetch::{self, parse_rustdoc_json};
use wtr::lookup;
use wtr::render;

fn load_rangemap(fixture: &str) -> fetch::Crate {
    let bytes = fs::read(fixture).unwrap();
    parse_rustdoc_json(&bytes, "rangemap").unwrap()
}

fn load_rangemap_v1_7_1() -> fetch::Crate {
    load_rangemap("tests/fixtures/rangemap-1.7.1.json")
}

fn load_rangemap_v1_6_0() -> fetch::Crate {
    load_rangemap("tests/fixtures/rangemap-1.6.0.json")
}

// ── Format version ──────────────────────────────────────────────────────

#[test]
fn v54_json_parses_successfully() {
    let krate = load_rangemap_v1_6_0();
    assert!(krate.crate_version.as_deref() == Some("1.6.0"));
}

#[test]
fn unsupported_old_format_version_gives_clear_error() {
    let json = br#"{"format_version": 1, "root": 0, "index": {}, "paths": {}}"#;
    let err = parse_rustdoc_json(json, "fake").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("too old"),
        "expected 'too old' error, got: {msg}"
    );
}

// ── Lookup (v57) ────────────────────────────────────────────────────────

#[test]
fn lookup_crate_root() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &[]).unwrap();
    assert!(matches!(
        result.item.inner,
        rustdoc_types::ItemEnum::Module(_)
    ));
}

#[test]
fn lookup_top_level_struct() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    assert!(matches!(
        result.item.inner,
        rustdoc_types::ItemEnum::Struct(_)
    ));
    assert_eq!(result.item.name.as_deref(), Some("RangeMap"));
}

#[test]
fn lookup_top_level_struct_rangeset() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["RangeSet".into()]).unwrap();
    assert!(matches!(
        result.item.inner,
        rustdoc_types::ItemEnum::Struct(_)
    ));
}

#[test]
fn lookup_method() {
    let krate = load_rangemap_v1_7_1();
    let result =
        lookup::lookup_item(&krate, &["RangeMap".into(), "insert".into()]).unwrap();
    assert!(matches!(
        result.item.inner,
        rustdoc_types::ItemEnum::Function(_)
    ));
    assert_eq!(result.item.name.as_deref(), Some("insert"));
}

#[test]
fn lookup_nonexistent_item_fails() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["NonExistent".into()]);
    assert!(result.is_err());
}

#[test]
fn lookup_nonexistent_method_fails() {
    let krate = load_rangemap_v1_7_1();
    let result =
        lookup::lookup_item(&krate, &["RangeMap".into(), "nonexistent_method".into()]);
    assert!(result.is_err());
}

// Re-exported items: RangeMap is defined in `map::RangeMap` but re-exported
// at the crate root. It should be findable via both paths.
#[test]
fn lookup_reexported_struct_via_submodule_path() {
    let krate = load_rangemap_v1_7_1();
    let result =
        lookup::lookup_item(&krate, &["map".into(), "RangeMap".into()]).unwrap();
    assert!(matches!(
        result.item.inner,
        rustdoc_types::ItemEnum::Struct(_)
    ));
    assert_eq!(result.item.name.as_deref(), Some("RangeMap"));
}

#[test]
fn lookup_reexported_trait_via_submodule_path() {
    let krate = load_rangemap_v1_7_1();
    let result =
        lookup::lookup_item(&krate, &["std_ext".into(), "StepLite".into()]).unwrap();
    assert_eq!(result.item.name.as_deref(), Some("StepLite"));
}

// ── Module walk ─────────────────────────────────────────────────────────

#[test]
fn module_walk_finds_reexported_struct() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::find_by_module_walk(&krate, &["RangeMap".into()]);
    let result = result.expect("should find RangeMap via module walk");
    assert!(matches!(
        result.item.inner,
        rustdoc_types::ItemEnum::Struct(_)
    ));
    assert_eq!(result.item.name.as_deref(), Some("RangeMap"));
}

#[test]
fn module_walk_finds_item_in_submodule() {
    let krate = load_rangemap_v1_7_1();
    let result =
        lookup::find_by_module_walk(&krate, &["map".into(), "RangeMap".into()]);
    let result = result.expect("should find map::RangeMap via module walk");
    assert!(matches!(
        result.item.inner,
        rustdoc_types::ItemEnum::Struct(_)
    ));
    assert_eq!(result.item.name.as_deref(), Some("RangeMap"));
}

#[test]
fn module_walk_returns_none_for_missing() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::find_by_module_walk(&krate, &["NonExistent".into()]);
    assert!(result.is_none());
}

#[test]
fn module_walk_reports_reexport_source() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::find_by_module_walk(&krate, &["RangeMap".into()])
        .expect("should find RangeMap");
    assert!(
        result.reexport_source.is_some(),
        "RangeMap is re-exported, should have reexport_source"
    );
    let source = result.reexport_source.unwrap();
    assert!(
        source.contains("map"),
        "reexport_source should reference the map module, got: {source}"
    );
}

// ── Lookup (v54) ────────────────────────────────────────────────────────

#[test]
fn v54_lookup_struct() {
    let krate = load_rangemap_v1_6_0();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    assert!(matches!(
        result.item.inner,
        rustdoc_types::ItemEnum::Struct(_)
    ));
    assert_eq!(result.item.name.as_deref(), Some("RangeMap"));
}

#[test]
fn v54_lookup_method() {
    let krate = load_rangemap_v1_6_0();
    let result =
        lookup::lookup_item(&krate, &["RangeMap".into(), "insert".into()]).unwrap();
    assert!(matches!(
        result.item.inner,
        rustdoc_types::ItemEnum::Function(_)
    ));
    assert_eq!(result.item.name.as_deref(), Some("insert"));
}

// ── find_methods / find_trait_impls ─────────────────────────────────────

#[test]
fn find_methods_returns_methods() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let methods = lookup::find_methods(&krate, result.item);
    assert!(!methods.is_empty(), "RangeMap should have methods");
    let names: Vec<_> = methods.iter().filter_map(|m| m.name.as_deref()).collect();
    assert!(names.contains(&"insert"), "should have insert method");
    assert!(names.contains(&"get"), "should have get method");
}

#[test]
fn find_trait_impls_returns_impls() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let impls = lookup::find_trait_impls(&krate, result.item);
    let trait_names: Vec<_> = impls.iter().map(|(_, name)| name.as_str()).collect();
    assert!(
        trait_names.iter().any(|n| n.contains("Clone")),
        "RangeMap should implement Clone, got: {trait_names:?}"
    );
}

// ── Rendering (v57) ─────────────────────────────────────────────────────

#[test]
fn render_struct_summary() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let output = render::render_item_summary(result.item, &krate);
    assert!(output.contains("struct RangeMap"), "output: {output}");
    assert!(
        output.contains("pub"),
        "should show pub visibility: {output}"
    );
}

#[test]
fn render_method_signature() {
    let krate = load_rangemap_v1_7_1();
    let result =
        lookup::lookup_item(&krate, &["RangeMap".into(), "insert".into()]).unwrap();
    let output = render::render_item_summary(result.item, &krate);
    assert!(output.contains("fn insert"), "output: {output}");
}

#[test]
fn render_methods_list() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let output = render::render_methods(result.item, &krate);
    assert!(output.contains("Methods for RangeMap"), "output: {output}");
    assert!(output.contains("fn insert"), "output: {output}");
    assert!(output.contains("fn get"), "output: {output}");
}

#[test]
fn render_full_includes_docs() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let output = render::render_item_full(result.item, &krate);
    let summary = render::render_item_summary(result.item, &krate);
    assert!(
        output.len() > summary.len(),
        "full output should be longer than summary"
    );
}

#[test]
fn render_trait_impls_output() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let output = render::render_trait_impls(result.item, &krate);
    assert!(
        output.contains("Trait implementations for RangeMap"),
        "output: {output}"
    );
}

// ── Rendering (v54) ─────────────────────────────────────────────────────

#[test]
fn v54_render_struct_summary() {
    let krate = load_rangemap_v1_6_0();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let output = render::render_item_summary(result.item, &krate);
    assert!(output.contains("struct RangeMap"), "output: {output}");
}

#[test]
fn v54_render_methods() {
    let krate = load_rangemap_v1_6_0();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let output = render::render_methods(result.item, &krate);
    assert!(output.contains("fn insert"), "output: {output}");
}

// ── Suggestions ─────────────────────────────────────────────────────────

#[test]
fn suggestions_for_struct() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let suggestions = render::render_suggestions(
        "rangemap",
        &["RangeMap".into()],
        result.item,
        &krate,
        false,
        false,
        false,
    );
    assert!(
        suggestions.contains("--methods"),
        "suggestions: {suggestions}"
    );
    assert!(suggestions.contains("--full"), "suggestions: {suggestions}");
    assert!(
        suggestions.contains("--traits"),
        "suggestions: {suggestions}"
    );
}

#[test]
fn suggestions_omit_used_flags() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let suggestions = render::render_suggestions(
        "rangemap",
        &["RangeMap".into()],
        result.item,
        &krate,
        false,
        true,
        false,
    );
    assert!(!suggestions.contains("--methods"), "should omit --methods");
    assert!(suggestions.contains("--full"), "should still have --full");
}

#[test]
fn suggestions_empty_when_all_used() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let suggestions = render::render_suggestions(
        "rangemap",
        &["RangeMap".into()],
        result.item,
        &krate,
        true,
        true,
        true,
    );
    assert!(suggestions.is_empty(), "should be empty: {suggestions}");
}

#[test]
fn no_suggestions_for_function() {
    let krate = load_rangemap_v1_7_1();
    let result =
        lookup::lookup_item(&krate, &["RangeMap".into(), "insert".into()]).unwrap();
    let suggestions = render::render_suggestions(
        "rangemap",
        &["RangeMap".into(), "insert".into()],
        result.item,
        &krate,
        false,
        false,
        false,
    );
    // Functions don't have --methods or --traits suggestions.
    assert!(
        !suggestions.contains("--methods"),
        "suggestions: {suggestions}"
    );
    assert!(
        !suggestions.contains("--traits"),
        "suggestions: {suggestions}"
    );
    // But should still suggest --full.
    assert!(suggestions.contains("--full"), "suggestions: {suggestions}");
}

// ── Search ──────────────────────────────────────────────────────────────

#[test]
fn search_finds_exact_match() {
    let krate = load_rangemap_v1_7_1();
    let results = lookup::search_items(&krate, "RangeMap");
    assert!(!results.is_empty(), "should find RangeMap");
    let has_rangemap = results
        .iter()
        .any(|r| r.item.name.as_deref() == Some("RangeMap") && r.exact);
    assert!(has_rangemap, "should have an exact match for RangeMap");
}

#[test]
fn search_finds_substring_match() {
    let krate = load_rangemap_v1_7_1();
    let results = lookup::search_items(&krate, "range");
    let names: Vec<_> = results
        .iter()
        .filter_map(|r| r.item.name.as_deref())
        .collect();
    assert!(
        names.iter().any(|n| *n == "RangeMap"),
        "should find RangeMap: {names:?}"
    );
    assert!(
        names.iter().any(|n| *n == "RangeSet"),
        "should find RangeSet: {names:?}"
    );
}

#[test]
fn search_is_case_insensitive() {
    let krate = load_rangemap_v1_7_1();
    let results = lookup::search_items(&krate, "rangemap");
    let names: Vec<_> = results
        .iter()
        .filter_map(|r| r.item.name.as_deref())
        .collect();
    assert!(
        names.contains(&"RangeMap"),
        "case-insensitive search should find RangeMap: {names:?}"
    );
}

#[test]
fn search_no_results() {
    let krate = load_rangemap_v1_7_1();
    let results = lookup::search_items(&krate, "nonexistent");
    assert!(results.is_empty(), "should find nothing: {results:?}", results = results.len());
}

// ── Module children ─────────────────────────────────────────────────────

#[test]
fn render_module_full_lists_children() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["map".into()]).unwrap();
    let output = render::render_item_full(result.item, &krate);
    assert!(
        output.contains("Structs:"),
        "should have Structs heading: {output}"
    );
    assert!(
        output.contains("RangeMap"),
        "should list RangeMap child: {output}"
    );
}

#[test]
fn render_crate_root_full_lists_children() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &[]).unwrap();
    let output = render::render_item_full(result.item, &krate);
    // The root module should list top-level items.
    assert!(
        output.contains("RangeMap") || output.contains("Modules:"),
        "root full output should list children: {output}"
    );
}

#[test]
fn suggestions_for_module_suggest_children() {
    let krate = load_rangemap_v1_7_1();
    let result = lookup::lookup_item(&krate, &["map".into()]).unwrap();
    let suggestions = render::render_suggestions(
        "rangemap",
        &["map".into()],
        result.item,
        &krate,
        true,
        false,
        false,
    );
    assert!(
        suggestions.contains("wtr rangemap::map::"),
        "should suggest child paths: {suggestions}"
    );
}
