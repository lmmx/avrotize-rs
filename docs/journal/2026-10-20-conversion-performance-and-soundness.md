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
