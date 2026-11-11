# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`dexpi`, a Rust library crate for reading, writing and validating the DEXPI 2.0 exchange
format. Keep it independent of any one application: domain logic for P&IDs, block flow
diagrams and so on belongs in the tools that use this crate.

## Commands

```sh
cargo test                          # everything, including doctests
cargo test --test roundtrip         # one test file
cargo test reference_pid -- --nocapture
cargo clippy --all-targets
cargo test --no-default-features   # without the bundled models
tools/vendor-spec.sh                # regenerate spec/ from upstream (needs git, python >= 3.12)
```

## Why the code is shaped this way

DEXPI 2.0 dropped Proteus XML for a generic "DEXPI XML". The schema (`spec/schema/`)
describes only a metamodel (`Model`, `Package`, `ConcreteClass`, `Object`, `Data`,
`References`, ...). The domain models (Core, Plant, Process, Builtin) are themselves DEXPI
XML documents in `spec/models/`. So:

- **No Rust type per DEXPI class.** The models are loaded at runtime, so a spec update is a
  data update, not a code change. `Models::dexpi_2_0()` (default feature `bundled-models`)
  compiles `spec/models` in with `include_str!`, so regenerating `spec/` updates it too.
- **One parser reads both models and instance documents.**
- **Two layers, one-way dependency.** `xml` is syntax only and must never depend on
  `model`. Module-private items use `pub(super)`, not `pub(crate)`, which keeps each layer's
  internals out of the other's reach.

## `xml` fidelity rules

Parse-then-write reproduces the input byte for byte, including attribute spelling and order,
inter-element whitespace, entity references, and `<X/>` versus `<X></X>`. The round-trip
tests assert this against every file in `spec/models/` and `spec/examples/`, including the
1.5 MB reference P&ID. Two rules follow, and breaking either defeats the point of the layer:

- **Never drop unrecognized input.** Comments, processing instructions and an XML declaration
  are preserved. CDATA and DTDs are errors, not silent omissions. DEXPI files begin directly
  with `<Model>` and no XML declaration, and that is what we write too.
- **Attributes are parsed on demand from the preserved raw text**, never stored alongside it,
  so the two representations cannot disagree.

## `model` scope

Packages, classes, properties and enumeration literals are all one `Item` type, because
references address them the same way: a dotted path through named children
(`Core/PhysicalQuantities.PhysicalQuantity.UnitType`). Property lookup must walk supertypes.
Almost every property is inherited, so a lookup that ignored inheritance would reject nearly
every valid document.

`validate` checks object types, property existence and kind, multiplicities, and that
references point at objects in the document. It does **not** check values against their
declared data types (union and bound types such as `QualifiedValue` make that a larger job).

## `spec/`

Vendored files. Regenerate them with the script, don't edit them by hand, and see
`spec/VERSIONS.md` for their provenance and quirks. Some of those quirks matter to callers:

- Model URIs (`https://data.dexpi.org/...`) do not resolve. They are namespace identifiers
  only, so never fetch them.
- `Builtin` is referenced without an `<Import>` and must always be loaded. `MetaData` is
  imported but was never published, so an unresolved import is a warning, not an error.
- `reference_pid.xml` (a P&ID on the Plant model) has exactly nine known conformance errors,
  pinned by a test. A *new* kind of diagnostic against it is more likely a bug here.
- `flash_separation_bfd.xml` is hand-authored, not published, and it validates clean. It is
  the worked example for the Process model.
- DEXPI diagram coordinates run downward, as in SVG.
