# Conversion performance and soundness

A dated log of the work after 0.1.2: one entry per PR, plus the experiments that
didn't pan out, so they aren't repeated. Newest entries go at the bottom.

## Method

**Harness.** `examples/bench_convert.rs` generates schemas in-process and times
`jsons_to_avro`:

| shape | what it is | why |
|---|---|---|
| `wide 2000` | 2,000 properties, each a `{language, value}` record | genson's label maps (#27) |
| `narrow 200` | one chain of 200 nested records | cost per level of depth |
| `deep 30` | 40 side-by-side chains of 30 nested records | depth with repetition (identical shapes) |
| `collide 200` | one key holding 200 different record shapes, at three depths | name collisions (#28) |
| `genson 1000` | 1,000 seeded random genson-like schemas | a mixed workload |

It uses only the public API, so the same file builds against any release.

**Rig.** `tools/bench/measure.sh <label> <dependency>` builds the harness as its own
crate against one avrotize version (a release from crates.io, or a path), then for
each shape reports:

- wall time: the median of 7 runs (noisy on a shared 4-core VM; read it as a guide);
- instructions (callgrind), allocated bytes and blocks (dhat): each is a one-run count
  minus a generate-only count (`runs = 0`), so they cover the conversion alone;
- peak heap (dhat `t-gmax`), which includes the input schema, the same for all versions;
- a hash of the output, which must match between versions.

Instructions are the primary number: they are deterministic, and wall time follows them.

**Validation.** A change only lands if the output is the same, checked four ways:

1. `cargo test -F cli`: the 69 CLI snapshots, with none updated;
2. a strict CLI differential: 3,000–4,000 random genson-like schemas through the
   `master` and branch binaries, byte-identical;
3. a library differential: 3,000 schemas, a fifth of them moving objects into `$defs`
   behind `$ref`, in both inline and split-top-level mode (6,000 outputs);
4. all 39 fixtures through both CLIs.

The differential scripts generate their schemas from a seed, like the harness.

## Baseline (2026-10-20)

0.1.1 and 0.1.2 from crates.io, built with avrotize's own regex features (`std`,
`unicode-perl`). Conversion-only counts:

| shape | 0.1.1 Ir | 0.1.2 Ir | 0.1.2 wall | 0.1.2 alloc | 0.1.1 → 0.1.2 peak |
|---|---|---|---|---|---|
| wide 2000 | 2,914M | 497M | 44.5 ms | 55.4 MB | 31.1 → 31.1 MB |
| narrow 200 | 1,462M | **1,612M** | 243 ms | 299 MB | 4.6 → **81.1 MB** |
| deep 30 | 2,631M | 1,483M | 176 ms | 219 MB | 21.4 → **62.4 MB** |
| collide 200 | 873M | 454M | 36.0 ms | 22.4 MB | 7.9 → 7.9 MB |
| genson 1000 | 2,966M | 396M | 44.7 ms | 61.0 MB | 7.3 → 7.6 MB |

#27's regex fix made 0.1.2 5–7× faster than 0.1.1 on most shapes, but on deep nesting
it gained little or lost. On a single deep chain it is slower than 0.1.1, with 17.7×
the peak heap. The output hashes match for wide, narrow and deep. They differ for
collide and genson: those schemas have colliding names, and #28 changed their output
on purpose.

## 2026-10-20 · Shape ids instead of cloned subtrees (`perf/shape-ids`)

**Cause.** #30's `disambiguate_named_types` cloned and stripped each named type's whole
subtree to compare it with earlier definitions of the same full name, and kept the
clones. Time and memory grew with nodes × depth.

**Change.** Intern each value's shape bottom-up, a technique known as *hash-consing*. An
object's shape is its sorted `(key, child id)` entries, without bookkeeping keys, so:

- two values get the same id exactly when the old comparison found them equal;
- object key order is ignored, as `Value` equality ignores it;
- `const` and `default` data keep their keys.

The intern map compares whole shapes, and a hash only picks the bucket, so a hash
collision cannot merge two different shapes. One survey pass records each named type's
id in document order, and the rename pass works on ids.

**Exactness.** `tests/disambiguation.rs` keeps 0.1.2's implementation as a reference
and requires byte-identical output on 3,000 generated trees. The generator includes:

- colliding names;
- shuffled keys;
- bookkeeping keys;
- `const`/`default` data, including data shaped like named types;
- echoes of earlier types with only bookkeeping changed.

Mutation-tested: each of these bugs makes the test fail:
- comparing keys in order;
- not stripping bookkeeping keys;
- stripping them inside data.

**Results** (conversion only; output hashes unchanged):

| shape | Ir 0.1.2 → this | peak heap |
|---|---|---|
| narrow 200 | 1,612M → 1,210M (−25%) | 81.1 → **4.6 MB** |
| deep 30 | 1,483M → 1,089M (−27%) | 62.4 → **21.4 MB** |
| collide 200 | 454M → 237M (−48%) | 7.9 → 7.9 MB |
| wide 2000 | 497M → 464M (−7%) | 31.1 → 31.1 MB |
| genson 1000 | 396M → 394M | 7.6 → 7.6 MB |

Peak heap is back to 0.1.1's level.

## 2026-10-20 · Negative results and pitfalls so far

- **Callgrind inclusive costs lie under recursion.** The first profile of `narrow 200`
  put 47% of instructions in `Value`'s `Display` and showed inclusive costs above
  100%. These come from cycles in the recursive call graph.
  - With the harness's output hash skipped there was no `Display` call at all.
  - Use self costs (`callgrind_annotate` without `--inclusive`), and dhat for allocation
    sites.
  - dhat truncates deep stacks: allocation sites inside recursive `clone` or `serialize`
    lose their avrotize frame, even with `--num-callers=60`.
- **The harness inflates counts.** Generating `deep 30` is a fifth of the process. Hence
  the generate-only baseline the rig subtracts.
- **A fast unkeyed hasher for the interner: rejected.**
  - SipHash on shapes is about 4.7% of instructions on `genson 1000`.
  - An FxHash-style hasher would save most of that, but the keys are taken from
    user-supplied schemas. A crafted schema could then collide every bucket and make
    the pass quadratic.
  - Not worth it for under 5%. serde_json's own `IndexMap` lookups, another ~6.6%, use
    SipHash and are out of our hands.
- **(2026-10-19) An Avro library as a dev-dependency: rejected.**
  - `apache-avro` 0.18 and 0.19 enable serde_json's `arbitrary_precision` through
    `bigdecimal`. Feature unification in test builds then changes how the CLI writes
    numbers, and a snapshot fails.
  - 0.17 avoids that, but needs Rust 1.85–1.89 through `libflate` and `uuid`, above
    the 1.82 MSRV.
  - `tests/collisions.rs` uses a small in-test checker instead; fastavro checks schemas
    outside the repo.

## 2026-10-20 · Moves instead of `json!` copies (`perf/no-json-macro-copies`)

**Cause.** `json!` turns every interpolated expression into a value with
`to_value(&expr)`, which serializes it into a fresh copy, even when the expression is
an owned `Value`. Wrapping a type in an array, map, record or field, or a nullable
`["null", t]` union, therefore deep-copied the whole nested type. In a chain, each
level copied everything below it, and optional fields did so twice. That was 48% of
the bytes allocated for `narrow 200`, which dhat showed as `serialize_entry` and
`Serializer` sites.

**Change.** Build those objects by moving values in. `structs::object` keeps `json!`'s
key order. `generic_type()` is now built once and cloned, rather than rebuilt with
`json!` on every call.

**Results** (output unchanged by every check in the method):

| shape | Ir before → after |
|---|---|
| narrow 200 | 1,210M → **454M** (−62%) |
| deep 30 | 1,089M → 526M (−52%) |
| collide 200 | 237M → 198M |
| wide 2000 | 464M → 423M |
| genson 1000 | 394M → 351M |

## 2026-10-20 · Clones nothing used (`perf/resolver-clones`)

**Cause.** Whole-schema copies made up about 60% of `wide 2000`'s bytes. A full copy of
the schema made at each of these sites:

- `find_schema_node` called on a clone of the list;
- the found root cloned again;
- the list cloned once more for `inline_dependencies_of`;
- `process_definition` cloning the item it registers.

The rest came from `adjust_resolved_dependencies`, which cloned **every record's
subtree** to call `swap_record_dependencies_above(record, &its_own_clone)`.

**Change**, each one dead or redundant work:

- **The swap call.** Its first check compares the node with its own clone, which always
  matches, so it returns `None` without changing anything. The call, its clone and the
  method are removed.
- **The record list** was only searched with `contains`, so it is now a `HashSet`, not a
  `Vec`.
- **`sort_messages_by_dependencies`** ended by adjusting a temporary clone of its result
  and discarding it. Removed.
- **The `jsons_to_avro` copies** are gone: it searches the list in place, and passes the
  list itself to `inline_dependencies_of`, which never modifies it.
- **`process_definition`** moves the registered item instead of cloning it.

**Results**: output unchanged, including the library differential with `$defs` in
inline and split mode, which is the path `sort_messages_by_dependencies` takes.

| shape | Ir before → after | peak heap |
|---|---|---|
| wide 2000 | 423M → 222M (−48%) | 31.1 → **11.4 MB** |
| narrow 200 | 454M → 118M (−74%) | 4.6 → 2.0 MB |
| deep 30 | 526M → 212M (−60%) | 21.4 → 8.0 MB |
| collide 200 | 198M → 149M | 7.9 → 3.0 MB |
| genson 1000 | 351M → 204M (−42%) | 7.6 → 7.6 MB |

**Open question, logged for the soundness work.** The discarded adjust is a porting
divergence. Python avrotize's `sort_messages_by_dependencies` calls
`adjust_resolved_dependencies(sorted_messages)` on the list it returns, and a Python
list is mutated in place. The Rust port adjusts a clone. So in `$defs` and split mode,
Rust output may keep repeated inline definitions that Python would turn into
references. Restoring the adjustment would change output, so it is not part of a
performance PR.

## 2026-10-20 · A character map instead of a regex (`perf/name-char-map`)

**Cause.** avrotize builds `regex` without its `perf-*` features. Matching therefore
runs on the PikeVM, the slowest engine, one character at a time. For
`avro_name`/`avro_namespace`'s `replace_all` that was 15–20% of instructions
(`PikeVM::epsilon_closure`, `search_imp`, `next`).

**Change.** A negated class `[^a-zA-Z0-9_]` matches exactly one character, so replacing
it with `_` is a character map. When every character is allowed, which is the usual
case, the string is copied unchanged. `tests/names.rs` (#29) already checks both
functions against 0.1.1's regex implementation, on generated strings including:

- ASCII, dots, dashes and whitespace;
- accented and CJK characters, emoji and combining marks;
- NUL, U+FFFF and `İ`.

It also passed once at 200,000 strings.

**Results** (output unchanged):

| shape | Ir before → after |
|---|---|
| narrow 200 | 118M → **25.5M** (−78%) |
| deep 30 | 212M → 113M (−46%) |
| wide 2000 | 222M → 173M (−22%) |
| genson 1000 | 204M → 163M (−20%) |
| collide 200 | 149M → 137M |

In a downstream build with regex's default features (polars-genson's), regex was
already faster, so this gains less there.

## 2026-10-20 · Renaming-pass fast paths (`perf/disambiguation-fast-paths`)

**Causes.**
- **Shapes interned when nothing can collide.** Every schema had every shape interned,
  even when each full name was unique and no rename was possible.
- **A quadratic search for a free name.** Each new shape of a name searched from `r_1`
  upwards, allocating a candidate name and a key per probe. M shapes under one name
  cost O(M²), which shows on `collide 200`.

**Changes.**
- **Count names first.** The pass collecting taken names already visits every named
  type. If each full name occurs once, it returns before hashing any shape.
- **Resume the suffix search.** Names are only ever added to the taken set, so the first
  free suffix for a name never decreases. Each family remembers where its search
  stopped and continues from there, giving the same name.

**Results** (output unchanged; `tests/disambiguation.rs` still matches 0.1.2):

| shape | Ir before → after |
|---|---|
| collide 200 | 137M → **42.2M** (−69%) |
| wide 2000 | 173M → 129M (−26%) |
| deep 30 | 113M → 83.6M (−26%) |
| narrow 200 | 25.5M → 18.2M (−29%) |
| genson 1000 | 163M → 136M (−17%) |

## 2026-10-20 · Move the root instead of copying it (`perf/root-move`)

**Cause.** `find_schema_node` returns a clone of the node it finds, and for the root
that is nearly the whole output. Its signature is public, so it stays as it is.

**Change.** When the root is the list's first item, `jsons_to_avro` takes it out of the
list instead of copying it. The search would have returned that item without looking
further, so its depth limit could not have triggered.

`inline_dependencies_of` only looks up the root's dependencies in the list, so removing
the root changes nothing, unless the root lists itself as a dependency. In that case
the old path is kept.

**Results** (output unchanged):

| shape | Ir before → after | peak heap |
|---|---|---|
| wide 2000 | 129M → 98.6M (−24%) | 11.3 → **6.7 MB** |
| deep 30 | 83.6M → 66.2M (−21%) | 8.0 → 4.7 MB |
| genson 1000 | 136M → 118M (−13%) | 7.6 → 7.1 MB |
| narrow 200 | 18.2M → 15.4M | 2.0 → 1.3 MB |
| collide 200 | 42.2M → 35.6M | 3.0 → 1.8 MB |

## 2026-10-20 · Where the performance stack ends up

Against the 0.1.2 release, conversion only, with avrotize's own regex features:

| shape | Ir | wall | allocated | peak heap |
|---|---|---|---|---|
| wide 2000 | 497M → 98.6M (**5.0×**) | 44.5 → 7.9 ms | 55.4 → 5.5 MB | 31.1 → 6.7 MB |
| narrow 200 | 1,612M → 15.4M (**105×**) | 243 → 1.6 ms | 299 → 2.1 MB | 81.1 → 1.3 MB |
| deep 30 | 1,483M → 66.2M (**22×**) | 176 → 9.6 ms | 219 → 4.8 MB | 62.4 → 4.7 MB |
| collide 200 | 454M → 35.6M (**13×**) | 36.0 → 2.6 ms | 22.4 → 1.9 MB | 7.9 → 1.8 MB |
| collide 1000 | 8,263M → 193M (**43×**) | 909 → 15 ms | 158 → 9.0 MB | 39.5 → 9.0 MB |
| genson 1000 | 396M → 118M (**3.3×**) | 44.7 → 11.9 ms | 61.0 → 8.4 MB | 7.6 → 7.1 MB |

The output hash is the same as 0.1.2's on every shape.

**Downstream build.** With regex's default features added (polars-genson's build),
the regex in 0.1.2 is faster, so the gains are a little smaller:

| shape | 0.1.2 Ir → stack Ir | wall |
|---|---|---|
| wide 2000 | 454M → 98.6M (4.6×) | 50.0 → 8.0 ms |
| narrow 200 | 1,519M → 15.4M (99×) | 248 → 2.2 ms |
| deep 30 | 1,387M → 66.2M (21×) | 169 → 8.7 ms |
| collide 200 | 444M → 35.6M (12.5×) | 34.1 → 3.0 ms |
| genson 1000 | 361M → 118M (3.1×) | 38.7 → 11.8 ms |

**Against 0.1.1,** the release polars-genson used before 0.1.2: 30× fewer instructions
on wide, 95× on narrow, 40× on deep, 25× on genson.

**What is left.** Of `genson 1000`'s 118M instructions, these are mostly:

- the allocator (malloc and free together, about a third);
- serde_json's `IndexMap` key hashing (SipHash, about 7%), which is inherent to
  `Value` with `preserve_order`;
- the shape interner's SipHash (see the rejected fast hasher above).

Nesting depth no longer multiplies cost. All five shapes now scale roughly linearly
with their size.

## Soundness method

The performance stack keeps the output byte-identical. The entries below change it on
purpose, one defect per PR, and each one says what changes for users.

**What "sound" means here.** For a JSON Schema and its Avro output:

1. the converter exits cleanly;
2. the output is valid Avro, both to fastavro and to `apache-avro` (Rust), which also
   checks names strictly;
3. a JSON instance that `jsonschema` accepts under the JSON Schema can be written with
   the Avro schema without loss. Every key lands in a record field or a map, and every
   value matches its type, a union branch or an enum symbol.

**Fuzzer.** `tools/soundness/run.sh <n> <seed>` generates random schemas covering the
#8 checklist:

- nested objects, arrays, `anyOf`/`oneOf`/`allOf`, `$defs` references;
- enums, consts, type lists, formats;
- `additionalProperties` (true, false or a schema), `patternProperties`, and
  min/maxProperties.

For each schema it draws instances and reports the first failure, as a tally of kinds.
`reduce.py` shrinks a failing schema to a minimal one. Each defect below was reduced
this way, and each fix comes with a reduced regression test in `tests/soundness.rs`.

The strict parser runs as its own crate. `apache-avro` can't be a dev-dependency: it
enables serde_json's `arbitrary_precision`, which changes how this crate's numbers
parse, and it needs a newer Rust than the MSRV.

**Checker.** `tests/common` checks every test's output against the rules the converter
broke:

- each full name is defined once, and every reference is defined (the fuzzer's
  parsers also check that the definition comes first);
- no union directly in a union, and no two members of the same type;
- unique, present field names and enum symbols;
- a named `fixed`.

**Differential.** For each fix, a script converts 1,500–3,000 random schemas with the
branch and its base, and checks that every changed output had the defect being fixed.
Changes anywhere else count as regressions.

## 2026-10-20 · Formats without a logical type keep their type (`fix/unknown-formats`)

**Defect.** `json_schema_primitive_to_avro_type` returned an empty union for any
format it didn't map: email, uri, hostname, int32, double and the rest. That type
holds nothing.

- As a field type it gave `[]`, or `["null", []]` (a union nested in a union).
- In a `oneOf`, the empty branch was spliced away, silently dropping that alternative.
- The `int64` → `long` mapping was dead code.

**Change.** A format only annotates its value, so the primitive type stays. Python
avrotize 3.x does the same; 2.x had this bug, which is why the `email` fixture's
reference output is `[]`. A primitive root still converts to no type at all, as in
Python.

**Validation.** Every changed output in the differential held an empty union or had
an unmapped format.

**Compatibility.** A field typed `[]` could never be written, so no working pipeline
depends on the old output. An `int64` integer is now a `long`.

## 2026-10-20 · Well-formed unions (`fix/union-members`)

**Defect.** Avro forbids a union directly inside a union, and two members of the same
type in one union. The converter broke both rules:

- An optional field whose type was already a union, such as an optional `oneOf`, was
  wrapped as `["null", [...]]`.
- `oneOf` and `anyOf` members were concatenated as they came. A repeated member stayed
  unless it was next to its twin, because the resolver's `Vec::dedup` only removes
  neighbours. Two arrays, or two maps, stayed two members.

**Change.** `unions::union_members`:

- splices nested unions in;
- drops repeated members;
- merges every array into one whose items are the union of theirs, and every map the
  same way by values.

The nullable wrapper adds `null` to a union instead of nesting it. Members that break
no rule are kept as they are, in their order.

**Validation.** On 3,000 random schemas, all 342 outputs that changed broke a union
rule before.

**Compatibility.** Those schemas were rejected by every Avro parser, so no working
pipeline depends on them.

## 2026-10-20 · Non-string consts (`fix/non-string-const`)

**Defect.** A `const` became an enum of its string values. A number, boolean, null,
object or array const therefore gave an enum with no symbols, which is invalid Avro
and holds nothing.

**Change.** A scalar const takes its value's type, as its JSON type would map:

- `int`, or `long` for an integer beyond 32 bits;
- `float`, `boolean` or `null`.

An object or array const takes the generic type, which can hold it. The field keeps
its `const` attribute. String consts are unchanged.

**Compatibility.** The changed outputs were all invalid before. Python avrotize makes
these enums of stringified symbols (`_7`, `_True`), which fit the data only after
translating it. The value's type needs no translation.

## 2026-10-20 · `$defs` references resolve to what each definition became (`fix/defs-references`)

**Defect.** A `$ref` to `#/$defs/X` became `"<namespace>.X"` where it was met. That
reference dangled or came in the wrong order:

- inside a nested record, `<namespace>` was the nested record's, not the root
  namespace where the definition is registered;
- `X` was the raw key, while the definition is named `avro_name(X)`, so a key like
  `my-rec` was never found;
- a definition that isn't a named type (a primitive, an array, an object without
  properties) is never registered, so nothing defined the name;
- a definition that refers to one defined after it came first, and a schema list must
  define a name before using it;
- a `$ref` in `items`, a union branch or `additionalProperties` had no handling, and
  became the generic "any" union;
- `$defs` was ignored whenever `definitions` was also present.

**Change.**

- Conversion leaves a `#/$defs/X` marker in place.
- Once every definition (from both containers) is registered, `jsons_to_avro` replaces
  each marker with the registered full name, or with the converted type when the
  definition isn't a named type.
- A stable pass after the dependency sort, `define_before_use`, moves a definition
  ahead of the first item that refers to it. A list already in a valid order is left
  unchanged.

**Negative result: recording the dependency.** The first attempt resolved markers to
names and recorded each name as a dependency, so the sort would order the list. But
the dependency lists also feed inlining. A new dependency, `"null"` among them (the
converter records primitive names too), changed which types were inlined in schemas
with no `$defs` at all. The dependency lists are therefore left alone, and the
ordering pass needs nothing from them.

**Validation.** On 3,000 random schemas with `$defs`, every changed output was either
invalid before or had a reference outside a property, where the old code put the
generic type.

**Compatibility.** A schema that only references named definitions from top-level
properties converts as before.

## 2026-10-20 · Recursive pointer references (`fix/recursive-pointer-refs`)

**Defect.** Pointer references (`#/definitions/X`, `#`, `#/properties/...`) were
expanded inline at each use, with no check for recursion. A definition that refers to
itself (a linked list, a tree), or a `$ref` to `#`, expanded until the stack
overflowed and the process aborted. `$defs` references were spared only because they
became named references.

**Change.** `refs::expand_once` tracks the references being expanded, on a
thread-local stack with a drop guard. A reference met again while it is being
expanded becomes:

- a reference to the definition, which the `$defs` resolution now resolves for
  `definitions` too;
- or, when it points anywhere else, the generic type.

Each definition is registered with its own reference already marked, so a recursive
definition refers to itself by name: `Node { next: Node }`.

The helpers live in a module outside crustrace's instrumented one, because its
tracing needs `Debug` closure arguments.

**Validation.** Schemas that don't recurse convert as before, byte-identical on 3,500
random schemas.

**Compatibility.** Schemas that recurse used to crash the process.

## 2026-10-20 · Pattern map field names (`fix/pattern-field-name`)

**Defect.** `patternProperties` become a map field named after its record. A record
with a property of the record's own name (`kind` in `kind`) therefore had two fields
with one name, which Avro rejects.

**Change.** When that name is already a field, the map field takes the first free
suffix (`kind_1`, ...). Otherwise it is named as before.

**Validation.** On 3,000 random schemas, all 29 changed outputs had a repeated field
name.

## 2026-10-20 · `const` and `default` values are data (`fix/disambiguation-skips-data`)

**Defect.** Two passes treated a `const` or `default` value as part of the schema:

- `disambiguate_named_types` renamed data shaped like a named type (an object with
  `type: record` and a `name`) when its full name collided;
- `adjust_resolved_dependencies` replaced such data with a reference when its full
  name had been seen, and ran `Vec::dedup` on every array it walked, so a const of
  `[1, 1, 2]` became `[1, 2]`.

**Change.** Both skip `const` and `default` values, as `strip_bookkeeping` already
did. The 0.1.2 reference copy in `tests/disambiguation.rs` gets the same one-line
change, so it still checks everything else byte for byte.

**Compatibility.** Only values that were corrupted change.

## 2026-10-20 · Type lists with null keep null (`fix/nullable-type-lists`)

**Defect.** `{"type": ["boolean", "null"]}` converted to `"boolean"`. The null was
dropped, and added back only when the field was optional. So a required field, an
array item or a union branch with a nullable type list could not hold null, although
the JSON Schema allows it. Lists of three or more types dropped null the same way.
Python avrotize has the same bug.

**Change.** The other types are converted, then `null` is added with `union_members`.
Optional fields are unchanged, as they already had `null` first. A pattern map's values
are flattened the same way, so a pattern whose type is a union (from a type list or
`anyOf`) is no longer a union nested in the map's values.

**Validation.** Every changed output in the differential came from a type list with
null.

**Compatibility: this changes valid output.** A required nullable field, which genson
infers for a key present in every row but sometimes null, goes from `"string"` to
`["null", "string"]`. Data with a null there could not be written before, but
consumers that read the field's type see a union now.

## 2026-10-20 · `additionalProperties: true` holds any values (`fix/additional-properties-true`)

**Defect.** An object whose only properties are additional ones
(`additionalProperties: true`) became a map of strings, despite the code comment saying
"any values". A number, object or array among them could not be written.

**Change.** It becomes a map of the generic type, as an object without properties and
Python avrotize already do.

**Compatibility: this changes valid output.** The map's values go from `string` to the
generic union, which still includes `string`.

## 2026-10-20 · Objects without properties hold any properties (`fix/object-without-properties`, #12)

**Defect.** #12 reproduces on the stack's base. Any object schema with none of
`properties`, `patternProperties` and `additionalProperties` became a record with no
fields, which drops every key of the data. That includes `{"type": "object"}`, and
the #12 fixtures, which have only `minProperties` or `maxProperties`.

At the root, the empty record was also named like its wrapper, and #30 then renamed it
`document_1`. That is the snapshot change #30's review classified as a pre-existing
defect.

**Change.** Such an object becomes a map of the generic type, as JSON Schema allows any
properties there, and as Python avrotize does. An object with
`additionalProperties: false` (which must be empty), or with an explicit
`properties: {}`, stays a record, as in Python. `minProperties` and `maxProperties`
have no Avro equivalent and are dropped, as before.

The `object_maxproperties` and `object_minproperties` fixtures, which the CLI tests
skipped until now, convert exactly to their Python reference output. Their snapshots
are added, with empty diffs.

**Compatibility: this changes valid output.** genson infers `{"type": "object"}` for
an empty JSON object, so a field that was an empty record is now a map. Data with keys
there could not be written before.

## 2026-10-20 · Unions stay well formed after a reference resolves (`fix/refs-union-members`)

**Defect.** `union_members` runs during conversion, while a `$defs` reference is still
a marker. A reference that resolved to an inlined type could then repeat another
member: `anyOf [{$ref: Str}, {type: string}]` gave two strings. It could also be a union
itself.

**Change.** Each union in which a reference was resolved is normalized again.

## 2026-10-20 · `date-time` and `duration` (`fix/date-time-and-duration`)

**Defect.**

- `date-time` became an `int` date, which drops the time of day.
- `duration` became a `fixed` type without a name. The specification requires a name,
  and strict parsers reject an unnamed fixed.

**Change.**

- `date-time` becomes a `timestamp-millis` long, as in Python avrotize.
- A duration is a fixed type named `duration`, in its type's namespace.
- Dependency resolution now handles fixed types as it does records and enums, so a
  later identical duration becomes a reference, and a different type with the same
  full name is renamed (`duration_1`).

**Validation.** On 1,500 random schemas (with `duration` added to the generator's
formats), all 110 changed outputs had one of the two formats, and the strict parser
accepted every new output.

**Compatibility: this changes valid output.** A `date-time` field goes from an
`int`/`date` to a `long`/`timestamp-millis`. Writers translate both from strings, but
consumers see a new logical type.

## 2026-10-20 · Merging schemas of different types (`fix/merge-conflicting-types`)

**Defect.** When two schemas' types differ, `merge_json_schemas` widens the type to
a list of both, and then merged the second schema in whole. `merge_structures` saw
two types again and returned a list of the two schemas instead of their merge. A third
schema merged into that list panicked on `unwrap`. A root or `$defs` `allOf` of three
members of different types (`string`, `object`, `object`) reached it on the stack's
base and on 0.1.2.

**Change.** The rest of the schema is merged without its type, as Python avrotize
does, which gives one schema with a type list. When the types are equal, merging the
type again changed nothing, so that path is unaffected. Only merges that returned a
list, or panicked, change.

**Validation.** The fuzzer only puts `allOf` in properties, where (until the next
entry) a merge never saw two types, so its 1,500-schema differential shows no change.
The regression test is the reduced `$defs` case. The next entry's fuzz run, which
reaches these merges through properties, found the panic.

## 2026-10-20 · `allOf` in a property (`fix/all-of-members`)

**Defect.** Found by following up on the remaining `union` failures after the entries
above. `json_type_to_avro_type` handles an `allOf` met in a property, an array item or
a union branch, but it merged the schema with nothing but itself, `allOf` included.
Each step therefore recursed on the same schema until the depth limit (40) gave up
with the generic type, printing `Maximum recursion depth reached`. Every field was
lost, and the warning was the only sign. `allOf` at the root or in a definition goes
through `process_definition`, which merged correctly.

**Change.** The schema without its `allOf` is merged with each member, as
`process_definition` and Python avrotize do. A member's local `$ref` is merged with
what it points at, which is the usual extension pattern
(`allOf: [{$ref: Base}, {properties: ...}]`). That loop is bounded, as a reference may
lead back to itself. Such a definition still ends at the depth limit with the generic
type, as in Python.

**Validation.**

- On 1,500 random schemas, all 181 changed outputs have an `allOf`, and the strict
  parser accepts every new output. For `allOf [{$ref: Base}, {...}]` the output now
  matches Python's record.
- The first fuzz run of this fix crashed 4 times out of 2,000. Merging real members
  now reaches type conflicts, and with them the panic in the entry above. That entry
  is fixed first, and the fuzz run on this branch has no crashes.

**Compatibility: this changes valid output.** A property with `allOf` goes from the
generic union to a record with the merged fields. The fuzzer's `ok` count stays flat
(736 → 735). Those properties were accepted, shallowly, by the generic type, and are
now records. Like every converted record they are closed, so instances with extra
keys move from `ok` to `extra` (see the limitations below).
