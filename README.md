# dexpi

A Rust library for the DEXPI 2.0 exchange format.

- `dexpi::xml`: parses and writes DEXPI XML. Parse-then-write reproduces the input byte
  for byte, including formatting, attribute order and entity spelling.
- `dexpi::model`: loads the DEXPI 2.0 information models (Core, Plant, Process,
  Builtin) and validates documents against them.

```rust
use std::path::Path;

let models = dexpi::model::Models::load_from_dir(Path::new("spec/models"))?;
let text = std::fs::read_to_string("spec/examples/reference_pid.xml")?;
let document = dexpi::xml::parse(&text)?;
for diagnostic in models.validate(&document)? {
    println!("{diagnostic}");
}
assert_eq!(dexpi::xml::write(&document), text);
```

`spec/` holds the vendored DEXPI 2.0 models, schema and examples. See
[`spec/VERSIONS.md`](spec/VERSIONS.md) for their provenance and known quirks, and
regenerate them with `tools/vendor-spec.sh` (needs git and Python 3.12 or newer).

## License

Copyright 2026 Olaf Trygve Berglihn.

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. The vendored DEXPI specification files under
`spec/` are published by DEXPI e.V. under their own Creative Commons licence.
