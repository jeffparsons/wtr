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
- Supports rustdoc JSON format versions 39–57, but older versions may be missing some fields. Crates whose docs were built with a format version outside this range will fail with a clear error.
- No search/fuzzy matching — you need to know the exact item path.

## License

This project is released into the public domain — see [UNLICENSE](UNLICENSE) for details.
