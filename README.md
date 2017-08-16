# conch-runtime

[![Crates.io](https://img.shields.io/crates/v/conch-runtime.svg)](https://crates.io/crates/conch-runtime)
[![Documentation](https://docs.rs/conch-runtime/badge.svg)](https://docs.rs/conch-runtime)
[![Documentation Master](https://img.shields.io/badge/docs-master-blue.svg)](https://ipetkov.github.io/conch-runtime)
[![Build Status](https://travis-ci.org/ipetkov/conch-runtime.svg?branch=master)](https://travis-ci.org/ipetkov/conch-runtime)
[![Build Status](https://img.shields.io/appveyor/ci/ipetkov/conch-runtime/master.svg)](https://ci.appveyor.com/project/ipetkov/conch-runtime)
[![Coverage](https://img.shields.io/codecov/c/github/ipetkov/conch-runtime/master.svg)](https://codecov.io/gh/ipetkov/conch-runtime)

A Rust library/runtime for executing Unix shell commands.

## Quick Start
First, add this to your `Cargo.toml`:

```toml
[dependencies]
conch-runtime = "0.1.0"
```

Next, you can get started with by looking at [the barebones shell example][shell-example].

## About
This library offers executing already parsed shell commands as defined by the
[POSIX.1-2008][POSIX] standard. This runtime attempts to remain agnostic to the
specific Abstract Syntax Tree format a parser could produce, as well as agnostic
to features supported by the OS to be as cross platform as possible.

Specifically implementations are provided for all the default AST nodes produced
by the [`conch-parser`][conch-prarser] crate. Unlike other Unix shells, this
library supports Windows<sup>1</sup> and can likely be extended for other
operating systems as well.

<sup>1</sup>Major features are reasonably supported in Windows to the extent
possible. Due to OS differences (e.g. async I/O models) and inherent implementation
exepectations of the shell programming language, certain features may require
additional runtime costs, or may be limited in nature (e.g. inheriting arbitrary
numbered file descriptors [other than stdio] is difficult/impossible due to the
way Windows addresses file handles).

[POSIX]: http://pubs.opengroup.org/onlinepubs/9699919799/
[conch-parser]: https://docs.rs/conch-parser
[shell-example]: examples/shell.rs

### Goals
* Provide efficient, modularized implementations of executing parts of the shell
programming language that can be used as a basis to build out other shell
features
* Execution implementations should be indepdendently reusable and agnostic
to specific AST representations
* Reasonable feature parity across operating systems to the extent possible
* Avoid assumptions of particular implementation details and allow the caller
to select appropriate tradeoffs to the extent possible

### Non-goals
* 100% POSIX.1-2008 compliance: the standard is used as a baseline for
implementation and features may be further added (or dropped) based on what
makes sense or is most useful
* Feature parity with all major shells: unless a specific feature is
widely used (and considered common) or another compelling reason exists
for inclusion. However, this is not to say that the library will never
support extensions for adding additional syntax features.
* A full/complete shell implementation: this library aims to be a stepping stone
to building out a complete shell without being one itself.

## Supported features
- [x] Conditional lists (`foo && bar || baz`)
- [x] Pipelines (`! foo | bar`)
- [ ] Jobs (`foo &`)
- [x] Compound commands
  - [x] Brace blocks (`{ foo; }`)
  - [x] Subshells (`$(foo)`)
  - [x] `for` / `case` / `if` / `while` / `until`
- [x] Function declarations
- [x] Redirections
   - [x] Heredocs
   - [ ] File descriptor inheritance for non-stdio descriptors
- [ ] Expansions
   - [x] Parameters (`$foo`, `$@`, etc.)
   - [x] Parameter substitutions (`${foo:-bar}`)
   - [ ] Glob/Path Expansions
   - [ ] Tilde expansions
   - [ ] Arithmetic substitutions
     - [x] Common arithmetic operations required by the [standard][POSIX-arith]
     - [x] Variable expansion
     - [ ] Other inner abitrary parameter/substitution expansion
- [x] Quoting (single, double, backticks, escaping)
- [ ] Builtin commands (e.g. `cd`, `echo`, etc.)
- [ ] Signal handling
- [ ] Alias resolution

[POSIX-arith]: http://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html#tag_18_06_04

## License
Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution
Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
