The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- Added `eval_redirects_or_var_assignments2` and `EvalRedirectOrVarAssig2` implementations
which behave similar to their predecessors, except they apply variables directly into the
environment and provide a `VarRestorer` when finished
- Added `eval_redirects_or_var_assignments_with_restorers` which allows evaluating any
`RedirectOrVarAssig` with a specified `RedirectEnvRestorer` and `VarEnvRestorer` instances

### Changed
### Deprecated
- Deprecated `eval_redirects_or_var_assignments`, `eval_redirects_or_cmd_words_with_restorer`
and `EvalRedirectOrVarAssig`: the existing implementation does not handle referencing earlier
variable assignments (e.g. `var1=foo var2=${bar:-$var1} env`) but cannot be ammended without
introducing breaking changes

### Removed
### Fixed
### Security
### Breaking

## [0.1.1] - 2017-09-13
### Added
- Added `RedirectEnvRestorer` trait to abstract over `RedirectRestorer` and other implementations
- Added `spawn_with_local_redirections_and_restorer` to allow specifying a specific `RedirectEnvRestorer` implementation
- Added `VarEnvRestorer` trait to abstract over `VarRestorer` and other implementations

### Changed
- `eval_redirects_or_cmd_words_with_restorer` is now generic over a `RedirectEnvRestorer`
- `EvalRedirectOrCmdWordError` is also generic over any `RedirectEnvRestorer`, but defaults to `RedirectRestorer` for backward compatibility
- `eval_redirects_or_var_assignments_with_restorer` is now generic over a `RedirectEnvRestorer`
- `EvalRedirectOrVarAssig` is also generic over any `RedirectEnvRestorer`, but defaults to `RedirectRestorer` for backward compatibility
- `LocalRedirections` is also generic over any `RedirectEnvRestorer`, but defaults to `RedirectRestorer` for backward compatibility

### Deprecated
- Deprecated most of the direct methods on `RedirectRestorer` in favor of the `RedirectEnvRestorer` trait
- Deprecated most of the direct methods on `VarRestorer` in favor of the `VarEnvRestorer` trait

## [0.1.0] - 2017-08-21
- First release!

[Unreleased]: https://github.com/ipetkov/conch-runtime/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/ipetkov/conch-runtime/compare/v0.1.1...HEAD
[0.1.0]: https://github.com/ipetkov/conch-runtime/compare/v0.1.0...v0.1.1
