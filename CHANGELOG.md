The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
### Changed
### Deprecated
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
