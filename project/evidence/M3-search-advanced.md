# M3 Advanced Search Evidence

Date: 2026-06-23

## Implemented Slice

- `ResourcePredicate` composes exact, contains, glob, or regular-expression names with resource kinds, byte-size bounds, modification-time bounds, read-only and executable attributes, hidden policy, ignore policy, and optional content matching.
- Content matching supports literal text, regular expressions, and hexadecimal byte sequences with automatic, UTF-8, UTF-16LE, UTF-16BE, or Latin-1 decoding.
- Predicate validation reports the responsible field for invalid name/content regular expressions, hexadecimal byte strings, reversed size bounds, and reversed date bounds.
- Advanced semantics use predicate schema version 2 while version 1 glob and literal-text documents remain readable; version-mismatched advanced fields are rejected.
- The `Alt+F7` dialog exposes every criterion in one request. It accepts binary size suffixes and either ISO calendar dates or Unix milliseconds while preserving the overlay on invalid input.

## Automated Evidence

- `near-search` metadata tests prove name regex, kind, size, date, read-only, executable, and hidden criteria all accept one matching resource together and reject reversed ranges.
- Real-provider tests prove UTF-16LE regular-expression content search and raw hexadecimal search select the expected files.
- The Far workspace test submits all advanced dialog fields together, receives one exact source-backed result, and proves an invalid regex produces a field-specific diagnostic before replacing the current panel.

## Requirement Status

- `REQ-SEARCH-001` covers the complete typed advanced predicate and diagnostic contract.
- `FAR-SEARCH-002` is verified.
