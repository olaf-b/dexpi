# dexpi

A Rust library for the DEXPI 2.0 exchange format.

- `dexpi::xml`: parses and writes DEXPI XML. Parse-then-write reproduces the input byte
  for byte, including formatting, attribute order and entity spelling.
- `dexpi::model`: loads the DEXPI 2.0 information models (Core, Plant, Process,
  Builtin) and validates documents against them.

The DEXPI 2.0 models are compiled in, so validating a document takes no extra files:

```rust
let text = std::fs::read_to_string("my_pid.xml")?;
let document = dexpi::xml::parse(&text)?;

let models = dexpi::model::Models::dexpi_2_0();
for diagnostic in models.validate(&document)? {
    println!("{diagnostic}");
}
assert_eq!(dexpi::xml::write(&document), text);
```

The bundled models add about 1.2 MB to a program. To leave them out, disable the default
`bundled-models` feature and load the four model files (`Builtin.xml`, `Core.xml`,
`Plant.xml`, `Process.xml`) with `Models::load_from_dir` instead.

`spec/` holds the vendored DEXPI 2.0 models, schema and examples. See
[`spec/VERSIONS.md`](spec/VERSIONS.md) for their provenance and known quirks, and
regenerate them with `tools/vendor-spec.sh` (needs git and Python 3.12 or newer).

## License

Copyright 2026 Olaf Trygve Berglihn.

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

The DEXPI specification files vendored under `spec/` are published by DEXPI e.V. under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). See [`spec/LICENSE`](spec/LICENSE).
