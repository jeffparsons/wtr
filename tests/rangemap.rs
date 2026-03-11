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
    let item = lookup::lookup_item(&krate, &[]).unwrap();
    assert!(matches!(item.inner, rustdoc_types::ItemEnum::Module(_)));
}

#[test]
fn lookup_top_level_struct() {
    let krate = load_rangemap_v1_7_1();
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    assert!(matches!(item.inner, rustdoc_types::ItemEnum::Struct(_)));
    assert_eq!(item.name.as_deref(), Some("RangeMap"));
}

#[test]
fn lookup_top_level_struct_rangeset() {
    let krate = load_rangemap_v1_7_1();
    let item = lookup::lookup_item(&krate, &["RangeSet".into()]).unwrap();
    assert!(matches!(item.inner, rustdoc_types::ItemEnum::Struct(_)));
}

#[test]
fn lookup_method() {
    let krate = load_rangemap_v1_7_1();
    let item = lookup::lookup_item(&krate, &["RangeMap".into(), "insert".into()]).unwrap();
    assert!(matches!(item.inner, rustdoc_types::ItemEnum::Function(_)));
    assert_eq!(item.name.as_deref(), Some("insert"));
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
    let result = lookup::lookup_item(&krate, &["RangeMap".into(), "nonexistent_method".into()]);
    assert!(result.is_err());
}

// ── Lookup (v54) ────────────────────────────────────────────────────────

#[test]
fn v54_lookup_struct() {
    let krate = load_rangemap_v1_6_0();
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    assert!(matches!(item.inner, rustdoc_types::ItemEnum::Struct(_)));
    assert_eq!(item.name.as_deref(), Some("RangeMap"));
}

#[test]
fn v54_lookup_method() {
    let krate = load_rangemap_v1_6_0();
    let item = lookup::lookup_item(&krate, &["RangeMap".into(), "insert".into()]).unwrap();
    assert!(matches!(item.inner, rustdoc_types::ItemEnum::Function(_)));
    assert_eq!(item.name.as_deref(), Some("insert"));
}

// ── find_methods / find_trait_impls ─────────────────────────────────────

#[test]
fn find_methods_returns_methods() {
    let krate = load_rangemap_v1_7_1();
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let methods = lookup::find_methods(&krate, item);
    assert!(!methods.is_empty(), "RangeMap should have methods");
    let names: Vec<_> = methods.iter().filter_map(|m| m.name.as_deref()).collect();
    assert!(names.contains(&"insert"), "should have insert method");
    assert!(names.contains(&"get"), "should have get method");
}

#[test]
fn find_trait_impls_returns_impls() {
    let krate = load_rangemap_v1_7_1();
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let impls = lookup::find_trait_impls(&krate, item);
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
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let output = render::render_item_summary(item, &krate);
    assert!(output.contains("struct RangeMap"), "output: {output}");
    assert!(
        output.contains("pub"),
        "should show pub visibility: {output}"
    );
}

#[test]
fn render_method_signature() {
    let krate = load_rangemap_v1_7_1();
    let item = lookup::lookup_item(&krate, &["RangeMap".into(), "insert".into()]).unwrap();
    let output = render::render_item_summary(item, &krate);
    assert!(output.contains("fn insert"), "output: {output}");
}

#[test]
fn render_methods_list() {
    let krate = load_rangemap_v1_7_1();
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let output = render::render_methods(item, &krate);
    assert!(output.contains("Methods for RangeMap"), "output: {output}");
    assert!(output.contains("fn insert"), "output: {output}");
    assert!(output.contains("fn get"), "output: {output}");
}

#[test]
fn render_full_includes_docs() {
    let krate = load_rangemap_v1_7_1();
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let output = render::render_item_full(item, &krate);
    let summary = render::render_item_summary(item, &krate);
    assert!(
        output.len() > summary.len(),
        "full output should be longer than summary"
    );
}

#[test]
fn render_trait_impls_output() {
    let krate = load_rangemap_v1_7_1();
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let output = render::render_trait_impls(item, &krate);
    assert!(
        output.contains("Trait implementations for RangeMap"),
        "output: {output}"
    );
}

// ── Rendering (v54) ─────────────────────────────────────────────────────

#[test]
fn v54_render_struct_summary() {
    let krate = load_rangemap_v1_6_0();
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let output = render::render_item_summary(item, &krate);
    assert!(output.contains("struct RangeMap"), "output: {output}");
}

#[test]
fn v54_render_methods() {
    let krate = load_rangemap_v1_6_0();
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let output = render::render_methods(item, &krate);
    assert!(output.contains("fn insert"), "output: {output}");
}

// ── Suggestions ─────────────────────────────────────────────────────────

#[test]
fn suggestions_for_struct() {
    let krate = load_rangemap_v1_7_1();
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let suggestions =
        render::render_suggestions("rangemap", &["RangeMap".into()], item, false, false, false);
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
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let suggestions =
        render::render_suggestions("rangemap", &["RangeMap".into()], item, false, true, false);
    assert!(!suggestions.contains("--methods"), "should omit --methods");
    assert!(suggestions.contains("--full"), "should still have --full");
}

#[test]
fn suggestions_empty_when_all_used() {
    let krate = load_rangemap_v1_7_1();
    let item = lookup::lookup_item(&krate, &["RangeMap".into()]).unwrap();
    let suggestions =
        render::render_suggestions("rangemap", &["RangeMap".into()], item, true, true, true);
    assert!(suggestions.is_empty(), "should be empty: {suggestions}");
}

#[test]
fn no_suggestions_for_function() {
    let krate = load_rangemap_v1_7_1();
    let item = lookup::lookup_item(&krate, &["RangeMap".into(), "insert".into()]).unwrap();
    let suggestions = render::render_suggestions(
        "rangemap",
        &["RangeMap".into(), "insert".into()],
        item,
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
