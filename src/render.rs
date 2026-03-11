use rustdoc_types::*;
use std::io::IsTerminal;

use crate::fetch;
use crate::lookup;

// ── Type rendering ──────────────────────────────────────────────────────

pub fn render_type(ty: &Type) -> String {
    match ty {
        Type::ResolvedPath(path) => {
            let mut s = path.path.clone();
            if let Some(ref args) = path.args {
                s.push_str(&render_generic_args(args));
            }
            s
        }
        Type::Generic(name) => name.clone(),
        Type::Primitive(name) => name.clone(),
        Type::BorrowedRef {
            lifetime,
            is_mutable,
            type_,
        } => {
            let mut s = String::from("&");
            if let Some(lt) = lifetime {
                s.push_str(lt);
                s.push(' ');
            }
            if *is_mutable {
                s.push_str("mut ");
            }
            s.push_str(&render_type(type_));
            s
        }
        Type::Tuple(types) => {
            let inner: Vec<_> = types.iter().map(render_type).collect();
            format!("({})", inner.join(", "))
        }
        Type::Slice(ty) => format!("[{}]", render_type(ty)),
        Type::Array { type_, len } => format!("[{}; {len}]", render_type(type_)),
        Type::RawPointer { is_mutable, type_ } => {
            if *is_mutable {
                format!("*mut {}", render_type(type_))
            } else {
                format!("*const {}", render_type(type_))
            }
        }
        Type::FunctionPointer(fp) => render_fn_pointer(fp),
        Type::ImplTrait(bounds) => {
            let rendered: Vec<_> = bounds.iter().filter_map(render_generic_bound).collect();
            format!("impl {}", rendered.join(" + "))
        }
        Type::DynTrait(dyn_trait) => {
            let rendered: Vec<_> = dyn_trait
                .traits
                .iter()
                .map(|pt| {
                    let mut s = pt.trait_.path.clone();
                    if let Some(ref args) = pt.trait_.args {
                        s.push_str(&render_generic_args(args));
                    }
                    s
                })
                .collect();
            let mut s = format!("dyn {}", rendered.join(" + "));
            if let Some(ref lt) = dyn_trait.lifetime {
                s.push_str(&format!(" + {lt}"));
            }
            s
        }
        Type::QualifiedPath {
            name,
            self_type,
            trait_,
            ..
        } => {
            if let Some(t) = trait_.as_ref().filter(|t| !t.path.is_empty()) {
                format!("<{} as {}>::{name}", render_type(self_type), t.path)
            } else {
                format!("{}::{name}", render_type(self_type))
            }
        }
        Type::Infer => "_".to_string(),
        Type::Pat { type_, .. } => render_type(type_),
    }
}

fn render_generic_args(args: &GenericArgs) -> String {
    match args {
        GenericArgs::AngleBracketed { args, constraints } => {
            let mut parts: Vec<String> = args.iter().map(render_generic_arg).collect();
            for c in constraints {
                let mut s = c.name.clone();
                match &c.binding {
                    AssocItemConstraintKind::Equality(term) => {
                        s.push_str(" = ");
                        s.push_str(&render_term(term));
                    }
                    AssocItemConstraintKind::Constraint(bounds) => {
                        s.push_str(": ");
                        let rendered: Vec<_> =
                            bounds.iter().filter_map(render_generic_bound).collect();
                        s.push_str(&rendered.join(" + "));
                    }
                }
                parts.push(s);
            }
            if parts.is_empty() {
                String::new()
            } else {
                format!("<{}>", parts.join(", "))
            }
        }
        GenericArgs::Parenthesized { inputs, output } => {
            let args: Vec<_> = inputs.iter().map(render_type).collect();
            let mut s = format!("({})", args.join(", "));
            if let Some(out) = output {
                s.push_str(&format!(" -> {}", render_type(out)));
            }
            s
        }
        GenericArgs::ReturnTypeNotation => "(..)".to_string(),
    }
}

fn render_generic_arg(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Lifetime(lt) => lt.clone(),
        GenericArg::Type(ty) => render_type(ty),
        GenericArg::Const(c) => c.value.clone().unwrap_or_else(|| "_".to_string()),
        GenericArg::Infer => "_".to_string(),
    }
}

fn render_term(term: &Term) -> String {
    match term {
        Term::Type(ty) => render_type(ty),
        Term::Constant(c) => c.value.clone().unwrap_or_else(|| "_".to_string()),
    }
}

fn render_generic_bound(bound: &GenericBound) -> Option<String> {
    match bound {
        GenericBound::TraitBound {
            trait_, modifier, ..
        } => {
            let mut s = String::new();
            match modifier {
                TraitBoundModifier::Maybe => s.push('?'),
                TraitBoundModifier::MaybeConst => s.push_str("~const "),
                TraitBoundModifier::None => {}
            }
            s.push_str(&trait_.path);
            if let Some(ref args) = trait_.args {
                s.push_str(&render_generic_args(args));
            }
            Some(s)
        }
        GenericBound::Outlives(lt) => Some(lt.clone()),
        GenericBound::Use(_) => None,
    }
}

fn render_fn_pointer(fp: &FunctionPointer) -> String {
    let args: Vec<_> = fp
        .sig
        .inputs
        .iter()
        .map(|(_, ty)| render_type(ty))
        .collect();
    let mut s = format!("fn({})", args.join(", "));
    if let Some(ref out) = fp.sig.output {
        s.push_str(&format!(" -> {}", render_type(out)));
    }
    s
}

// ── Generics rendering ─────────────────────────────────────────────────

pub fn render_generics_params(generics: &Generics) -> String {
    if generics.params.is_empty() {
        return String::new();
    }
    let params: Vec<_> = generics
        .params
        .iter()
        .filter_map(|p| match &p.kind {
            GenericParamDefKind::Lifetime { outlives } => {
                let mut s = p.name.clone();
                if !outlives.is_empty() {
                    s.push_str(": ");
                    s.push_str(&outlives.join(" + "));
                }
                Some(s)
            }
            GenericParamDefKind::Type {
                bounds,
                default,
                is_synthetic,
            } => {
                if *is_synthetic {
                    return None;
                }
                let mut s = p.name.clone();
                if !bounds.is_empty() {
                    let rendered: Vec<_> = bounds.iter().filter_map(render_generic_bound).collect();
                    if !rendered.is_empty() {
                        s.push_str(": ");
                        s.push_str(&rendered.join(" + "));
                    }
                }
                if let Some(def) = default {
                    s.push_str(" = ");
                    s.push_str(&render_type(def));
                }
                Some(s)
            }
            GenericParamDefKind::Const { type_, default } => {
                let mut s = format!("const {}: {}", p.name, render_type(type_));
                if let Some(def) = default {
                    s.push_str(&format!(" = {def}"));
                }
                Some(s)
            }
        })
        .collect();
    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

pub fn render_where_clause(generics: &Generics) -> String {
    if generics.where_predicates.is_empty() {
        return String::new();
    }
    let predicates: Vec<_> = generics
        .where_predicates
        .iter()
        .filter_map(|pred| match pred {
            WherePredicate::BoundPredicate { type_, bounds, .. } => {
                let rendered: Vec<_> = bounds.iter().filter_map(render_generic_bound).collect();
                if rendered.is_empty() {
                    None
                } else {
                    Some(format!("{}: {}", render_type(type_), rendered.join(" + ")))
                }
            }
            WherePredicate::LifetimePredicate { lifetime, outlives } => {
                Some(format!("{lifetime}: {}", outlives.join(" + ")))
            }
            WherePredicate::EqPredicate { lhs, rhs } => {
                Some(format!("{} = {}", render_type(lhs), render_term(rhs)))
            }
        })
        .collect();
    if predicates.is_empty() {
        String::new()
    } else {
        format!("\nwhere\n    {}", predicates.join(",\n    "))
    }
}

// ── Function signature rendering ────────────────────────────────────────

pub fn render_function_sig(item: &Item, func: &Function) -> String {
    let mut s = render_visibility(&item.visibility);

    if func.header.is_const {
        s.push_str("const ");
    }
    if func.header.is_async {
        s.push_str("async ");
    }
    if func.header.is_unsafe {
        s.push_str("unsafe ");
    }

    s.push_str("fn ");
    s.push_str(item.name.as_deref().unwrap_or("_"));
    s.push_str(&render_generics_params(&func.generics));

    let args: Vec<_> = func
        .sig
        .inputs
        .iter()
        .map(|(name, ty)| {
            let ty_str = render_type(ty);
            // Self parameters.
            if name == "self" {
                if ty_str == "Self" {
                    "self".to_string()
                } else if ty_str == "&Self" {
                    "&self".to_string()
                } else if ty_str == "&mut Self" {
                    "&mut self".to_string()
                } else {
                    format!("self: {ty_str}")
                }
            } else {
                format!("{name}: {ty_str}")
            }
        })
        .collect();
    s.push('(');
    s.push_str(&args.join(", "));
    s.push(')');

    if let Some(ref output) = func.sig.output {
        s.push_str(&format!(" -> {}", render_type(output)));
    }

    s.push_str(&render_where_clause(&func.generics));

    s
}

fn render_visibility(vis: &Visibility) -> String {
    match vis {
        Visibility::Public => "pub ".to_string(),
        Visibility::Crate => "pub(crate) ".to_string(),
        Visibility::Restricted { path, .. } => format!("pub(in {path}) "),
        Visibility::Default => String::new(),
    }
}

// ── Item rendering ──────────────────────────────────────────────────────

fn first_doc_line(docs: &Option<String>) -> Option<&str> {
    docs.as_deref()
        .and_then(|d| d.lines().map(|l| l.trim()).find(|l| !l.is_empty()))
}

/// Render an item in summary mode (default).
pub fn render_item_summary(item: &Item, krate: &fetch::Crate) -> String {
    let mut out = String::new();

    match &item.inner {
        ItemEnum::Function(func) => {
            out.push_str(&render_function_sig(item, func));
            out.push('\n');
        }
        ItemEnum::Struct(s) => {
            out.push_str(&render_visibility(&item.visibility));
            out.push_str("struct ");
            out.push_str(item.name.as_deref().unwrap_or("_"));
            out.push_str(&render_generics_params(&s.generics));
            match &s.kind {
                StructKind::Unit => {}
                StructKind::Tuple(_) => out.push_str("(...)"),
                StructKind::Plain { .. } => out.push_str(" { /* fields */ }"),
            }
            out.push_str(&render_where_clause(&s.generics));
            out.push('\n');
        }
        ItemEnum::Enum(e) => {
            out.push_str(&render_visibility(&item.visibility));
            out.push_str("enum ");
            out.push_str(item.name.as_deref().unwrap_or("_"));
            out.push_str(&render_generics_params(&e.generics));
            // Show variant names inline.
            let variant_names: Vec<_> = e
                .variants
                .iter()
                .filter_map(|id| krate.index.get(id))
                .filter_map(|v| v.name.as_deref())
                .collect();
            if !variant_names.is_empty() {
                let preview: Vec<_> = variant_names.iter().take(5).copied().collect();
                let remaining = variant_names.len().saturating_sub(5);
                out.push_str(" { ");
                out.push_str(&preview.join(", "));
                if remaining > 0 {
                    out.push_str(&format!(", /* ... {remaining} more */"));
                }
                out.push_str(" }");
            }
            out.push_str(&render_where_clause(&e.generics));
            out.push('\n');
        }
        ItemEnum::Trait(t) => {
            out.push_str(&render_visibility(&item.visibility));
            if t.is_unsafe {
                out.push_str("unsafe ");
            }
            out.push_str("trait ");
            out.push_str(item.name.as_deref().unwrap_or("_"));
            out.push_str(&render_generics_params(&t.generics));
            if !t.bounds.is_empty() {
                let rendered: Vec<_> = t.bounds.iter().filter_map(render_generic_bound).collect();
                if !rendered.is_empty() {
                    out.push_str(": ");
                    out.push_str(&rendered.join(" + "));
                }
            }
            out.push_str(&render_where_clause(&t.generics));
            out.push_str(" {\n");
            // Show method signatures.
            for item_id in &t.items {
                if let Some(assoc_item) = krate.index.get(item_id)
                    && let ItemEnum::Function(ref func) = assoc_item.inner
                {
                    out.push_str("    ");
                    out.push_str(&render_function_sig(assoc_item, func));
                    out.push_str(";\n");
                }
            }
            out.push_str("}\n");
        }
        ItemEnum::TypeAlias(ta) => {
            out.push_str(&render_visibility(&item.visibility));
            out.push_str("type ");
            out.push_str(item.name.as_deref().unwrap_or("_"));
            out.push_str(&render_generics_params(&ta.generics));
            out.push_str(&format!(" = {}", render_type(&ta.type_)));
            out.push_str(";\n");
        }
        ItemEnum::Constant { type_, const_ } => {
            out.push_str(&render_visibility(&item.visibility));
            out.push_str("const ");
            out.push_str(item.name.as_deref().unwrap_or("_"));
            out.push_str(&format!(": {}", render_type(type_)));
            if let Some(ref val) = const_.value {
                out.push_str(&format!(" = {val}"));
            }
            out.push_str(";\n");
        }
        ItemEnum::Module(_) => {
            out.push_str(&render_visibility(&item.visibility));
            out.push_str("mod ");
            out.push_str(item.name.as_deref().unwrap_or("_"));
            out.push('\n');
        }
        _ => {
            if let Some(name) = &item.name {
                out.push_str(name);
                out.push('\n');
            }
        }
    }

    if let Some(line) = first_doc_line(&item.docs) {
        out.push('\n');
        out.push_str(line);
        out.push('\n');
    }

    out
}

/// Render full documentation for an item.
pub fn render_item_full(item: &Item, krate: &fetch::Crate) -> String {
    let mut out = render_item_summary(item, krate);

    // For full mode, replace the first doc line with the complete docs.
    if let Some(ref docs) = item.docs {
        // Remove the summary doc line we already added.
        if let Some(first_line) = first_doc_line(&Some(docs.clone()))
            && out.ends_with(&format!("{first_line}\n"))
        {
            let trim_len = first_line.len() + 1;
            out.truncate(out.len() - trim_len);
        }
        out.push_str(docs.trim());
        out.push('\n');
    }

    // Show fields for structs.
    if let ItemEnum::Struct(s) = &item.inner
        && let StructKind::Plain { fields, .. } = &s.kind
        && !fields.is_empty()
    {
        out.push_str("\nFields:\n");
        for field_id in fields {
            if let Some(field_item) = krate.index.get(field_id) {
                let name = field_item.name.as_deref().unwrap_or("_");
                if let ItemEnum::StructField(ref ty) = field_item.inner {
                    out.push_str(&format!("  {name}: {}\n", render_type(ty)));
                    if let Some(line) = first_doc_line(&field_item.docs) {
                        out.push_str(&format!("    {line}\n"));
                    }
                }
            }
        }
    }

    // Show variants for enums.
    if let ItemEnum::Enum(e) = &item.inner {
        out.push_str("\nVariants:\n");
        for variant_id in &e.variants {
            if let Some(variant_item) = krate.index.get(variant_id) {
                let name = variant_item.name.as_deref().unwrap_or("_");
                out.push_str(&format!("  {name}\n"));
                if let Some(line) = first_doc_line(&variant_item.docs) {
                    out.push_str(&format!("    {line}\n"));
                }
            }
        }
    }

    out
}

/// Render methods list for a type.
pub fn render_methods(item: &Item, krate: &fetch::Crate) -> String {
    let methods = lookup::find_methods(krate, item);
    if methods.is_empty() {
        return "No inherent methods found.\n".to_string();
    }

    let mut out = format!("Methods for {}:\n\n", item.name.as_deref().unwrap_or("_"));
    for method in &methods {
        if let ItemEnum::Function(ref func) = method.inner {
            out.push_str(&render_function_sig(method, func));
            out.push('\n');
            if let Some(line) = first_doc_line(&method.docs) {
                out.push_str(&format!("  {line}\n"));
            }
            out.push('\n');
        }
    }
    out
}

/// Render trait implementations for a type.
pub fn render_trait_impls(item: &Item, krate: &fetch::Crate) -> String {
    let impls = lookup::find_trait_impls(krate, item);
    if impls.is_empty() {
        return "No trait implementations found.\n".to_string();
    }

    let mut out = format!(
        "Trait implementations for {}:\n\n",
        item.name.as_deref().unwrap_or("_")
    );
    for (impl_item, trait_name) in &impls {
        if let ItemEnum::Impl(ref impl_data) = impl_item.inner {
            out.push_str(&format!("impl {trait_name}"));
            if !impl_data.generics.params.is_empty() {
                out.push_str(&render_generics_params(&impl_data.generics));
            }
            out.push_str(&format!(" for {}\n", render_type(&impl_data.for_)));
            // List methods provided.
            for assoc_id in &impl_data.items {
                if let Some(assoc_item) = krate.index.get(assoc_id)
                    && let ItemEnum::Function(ref func) = assoc_item.inner
                {
                    out.push_str(&format!("  {}\n", render_function_sig(assoc_item, func)));
                }
            }
            out.push('\n');
        }
    }
    out
}

/// Render suggestions for further commands.
pub fn render_suggestions(
    crate_name: &str,
    path: &[String],
    item: &Item,
    used_full: bool,
    used_methods: bool,
    used_traits: bool,
) -> String {
    let has_impls = matches!(
        &item.inner,
        ItemEnum::Struct(_) | ItemEnum::Enum(_) | ItemEnum::Union(_)
    );

    let query = if path.is_empty() {
        crate_name.to_string()
    } else {
        format!("{crate_name}::{}", path.join("::"))
    };

    let mut suggestions = Vec::new();

    if !used_methods && has_impls {
        suggestions.push(format!("  wtr {query} --methods    List methods"));
    }
    if !used_full {
        suggestions.push(format!("  wtr {query} --full       Full documentation"));
    }
    if !used_traits && has_impls {
        suggestions.push(format!("  wtr {query} --traits     Trait implementations"));
    }

    if suggestions.is_empty() {
        return String::new();
    }

    let mut out = String::from("\nSee more:\n");
    for s in &suggestions {
        out.push_str(s);
        out.push('\n');
    }
    out
}

fn use_color(no_color: bool) -> bool {
    !no_color && std::io::stdout().is_terminal() && std::env::var("NO_COLOR").is_err()
}

/// Print rendered output, dimming suggestions.
pub fn print_output(body: &str, suggestions: &str, no_color: bool) {
    print!("{body}");

    if !suggestions.is_empty() {
        if use_color(no_color) {
            let mut skin = termimad::MadSkin::default();
            skin.paragraph
                .set_fg(termimad::crossterm::style::Color::DarkGrey);
            skin.print_text(suggestions);
        } else {
            print!("{suggestions}");
        }
    }
}
