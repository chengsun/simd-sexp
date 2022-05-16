# simd-sexp

Some experiments with faster sexp parsing using SIMD.

This project is currently in a prototype stage. Don't use this for anything in
production. Expect things in this repo to change/break/disappear without
notice.

Things that currently exist and are maybe interesting/useful:

```
$ < test.sexp cargo run --release --bin select -- foo bar
$ dune exec benches/benches.exe
$ cargo bench parser
$ cargo test
$ dune test
```

(NB: the benches might fail because they reference some test data files, which
I haven't committed to this repo. They're quite big, so I don't want to check
them in. I haven't figured out where to put them yet.)

Lots of inspiration drawn from the excellent work done by the people behind
[simdjson](https://simdjson.org/). In simd-sexp there is a notion of "stage-1"
(SIMD) and "stage-2" (branchy) that's lifted verbatim from the simdjson paper.

This repository has both Rust and OCaml code. Most recently the prototyping has
focused on the Rust-side only, so the OCaml is starting to get a little stale.

Stage-1 is written purely in Rust. Stage-2 has multiple different
implementations (outputting various forms of tapes / trees), most in Rust
(although there does exist an stage-2 implementation in pure OCaml, outputting
the native OCaml `Sexp.t` type).

`cargo` knows about the Rust code only, whilst `dune` knows about both. In
particular `dune` knows how to invoke `cargo` as needed. However, the
`dune`-invoked `cargo` build artifacts live in the dune `_build` subdirectory;
they are not shared with those that are created by running `cargo` manually.
