# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/lmmx/avrotize-rs/compare/v0.1.2...v0.2.0) - 2026-10-20

### Fixed

- *(conversion)* oneOf and anyOf branches take on the schema around them ([#55](https://github.com/lmmx/avrotize-rs/pull/55))
- define a repeated named type once in a schema list ([#54](https://github.com/lmmx/avrotize-rs/pull/54))
- *(cli)* make the file name a valid namespace ([#53](https://github.com/lmmx/avrotize-rs/pull/53))
- *(conversion)* an allOf in a property merges its members ([#52](https://github.com/lmmx/avrotize-rs/pull/52))
- *(merging)* merging schemas of different types no longer panics ([#51](https://github.com/lmmx/avrotize-rs/pull/51))
- *(types)* date-time keeps the time, and duration is a named fixed type ([#50](https://github.com/lmmx/avrotize-rs/pull/50))
- *(refs)* normalize a union after resolving a definition reference in it ([#49](https://github.com/lmmx/avrotize-rs/pull/49))
- *(conversion)* an object without properties holds any properties ([#12](https://github.com/lmmx/avrotize-rs/pull/12)) ([#48](https://github.com/lmmx/avrotize-rs/pull/48))
- *(conversion)* additionalProperties: true holds values of any type ([#47](https://github.com/lmmx/avrotize-rs/pull/47))
- *(conversion)* a type list with null keeps null ([#46](https://github.com/lmmx/avrotize-rs/pull/46))
- leave const and default data alone when renaming and resolving types ([#45](https://github.com/lmmx/avrotize-rs/pull/45))
- *(conversion)* name a pattern map field so it doesn't repeat a property ([#44](https://github.com/lmmx/avrotize-rs/pull/44))
- *(refs)* a recursive pointer reference no longer overflows the stack ([#43](https://github.com/lmmx/avrotize-rs/pull/43))
- *(refs)* resolve $defs references to what each definition became ([#42](https://github.com/lmmx/avrotize-rs/pull/42))
- *(conversion)* a non-string const takes its value's type ([#41](https://github.com/lmmx/avrotize-rs/pull/41))
- *(unions)* no unions in unions, no repeated members, one array or map ([#40](https://github.com/lmmx/avrotize-rs/pull/40))
- *(types)* keep the primitive type for formats without a logical type ([#39](https://github.com/lmmx/avrotize-rs/pull/39))

### Other

- *(unions)* find a repeated union member by kind and hash, not by comparing all ([#57](https://github.com/lmmx/avrotize-rs/pull/57))
- *(converter)* move the root out of the list instead of copying it ([#38](https://github.com/lmmx/avrotize-rs/pull/38))
- *(postprocess)* renaming-pass fast paths (unique names; resumable suffix search) ([#37](https://github.com/lmmx/avrotize-rs/pull/37))
- *(names)* map disallowed characters to _ without a regex ([#36](https://github.com/lmmx/avrotize-rs/pull/36))
- *(resolver)* drop clones whose results were never used ([#35](https://github.com/lmmx/avrotize-rs/pull/35))
- *(converter)* move values into new types instead of copying them via json! ([#34](https://github.com/lmmx/avrotize-rs/pull/34))
- *(postprocess)* compare named type shapes by interned id, not by clone ([#33](https://github.com/lmmx/avrotize-rs/pull/33))
- *(release)* keep the tree clean in just ship; take an explicit version ([#56](https://github.com/lmmx/avrotize-rs/pull/56))
- *(cd)* vendor OpenSSL for the Linux cross targets ([#32](https://github.com/lmmx/avrotize-rs/pull/32))
- *(cd)* build macOS binaries on macos-15; add dry-run and tag backfill to manual runs ([#31](https://github.com/lmmx/avrotize-rs/pull/31))

## [0.1.2](https://github.com/lmmx/avrotize-rs/compare/v0.1.1...v0.1.2) - 2026-10-19

### Fixed

- distinct names for differently shaped records and enums; strip bookkeeping keys ([#30](https://github.com/lmmx/avrotize-rs/pull/30))

### Other

- *(clippy)* lint fixes
- compile name regexes once; drop duplicate name fns; fix unicode-perl panic ([#29](https://github.com/lmmx/avrotize-rs/pull/29))
- *(pre-commit)* autoupdate hooks ([#25](https://github.com/lmmx/avrotize-rs/pull/25))
- *(pre-commit)* autoupdate hooks ([#24](https://github.com/lmmx/avrotize-rs/pull/24))
- *(pre-commit)* autoupdate hooks ([#23](https://github.com/lmmx/avrotize-rs/pull/23))

## [0.1.1](https://github.com/lmmx/avrotize-rs/compare/v0.1.0...v0.1.1) - 2025-09-10

### Added

- convert map of records ([#22](https://github.com/lmmx/avrotize-rs/pull/22))

## [0.1.0](https://github.com/lmmx/avrotize-rs/releases/tag/v0.1.0) - 2025-09-05

### Added

- special case empty schema ([#16](https://github.com/lmmx/avrotize-rs/pull/16))
- array utility namespace and name ([#15](https://github.com/lmmx/avrotize-rs/pull/15))
- support default values ([#7](https://github.com/lmmx/avrotize-rs/pull/7))

### Fixed

- swap const/doc order ([#6](https://github.com/lmmx/avrotize-rs/pull/6))
- typo
- types are now nullable unions ([#3](https://github.com/lmmx/avrotize-rs/pull/3))
- fixture refactor ([#1](https://github.com/lmmx/avrotize-rs/pull/1))

### Other

- remove bad cat
- tracer arg lint, docs lints, type lints ([#20](https://github.com/lmmx/avrotize-rs/pull/20))
- crate docs (prepare for release) ([#19](https://github.com/lmmx/avrotize-rs/pull/19))
- clone release machinery
- roadmap link
- more ops ([#18](https://github.com/lmmx/avrotize-rs/pull/18))
- nullify unsupported ops ([#17](https://github.com/lmmx/avrotize-rs/pull/17))
- *(difft-all)* show source JSON schema
- full spec test cases; use snapshot of diffs to mark completion ([#14](https://github.com/lmmx/avrotize-rs/pull/14))
- README
- schema namespace fix ([#5](https://github.com/lmmx/avrotize-rs/pull/5))
- set up Justfile linters ([#4](https://github.com/lmmx/avrotize-rs/pull/4))
- verified example snapshot diff recipe ([#2](https://github.com/lmmx/avrotize-rs/pull/2))
- accept snapshot
- Initial commit
