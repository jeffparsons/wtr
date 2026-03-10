# wtr

> **Note:** This code was largely written by Claude (Anthropic).

Look up Rust crate documentation from [docs.rs](https://docs.rs) in your terminal.

Fetches and caches the rustdoc JSON for a crate, then renders type signatures, doc summaries, methods, and trait implementations.

## Usage

```
wtr jiff::Timestamp
wtr jiff::Timestamp --methods
wtr jiff::Timestamp --full
wtr jiff::Timestamp --traits
wtr jiff::Timestamp::now
```

## Limitations

- Only works with crates published to docs.rs after 2025-05-23 (when rustdoc JSON became available).
- Tied to a specific rustdoc JSON format version (currently v57). Crates whose docs were built with a different format version will fail with a clear error.
- No search/fuzzy matching — you need to know the exact item path.
