The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- Added a `EvalRedirectOrVarAssig2` implementation which behaves similar to its predecessor,
except it applies variables directly into the environment and provide a `VarRestorer` when finished
- Added `eval_redirects_or_var_assignments_with_restorers` which allows evaluating any
`RedirectOrVarAssig` with a specified `RedirectEnvRestorer` and `VarEnvRestorer` instances
- Added `VarEnvRestorer2` trait as a correction to the `VarEnvRestorer` interface in a
backwards compatible manner.
- Added `ShiftArgumentsEnvironment` as an interface for shifting positional arguments.
- Added a `spawn::builtin` module for hosting shell-builtin command implementations
- Added a builtin implementations for the following shell commands:
 - `shift`
 - `:`
 - `true`
 - `false`
 - `cd`
 - `pwd`
 - `echo`
- Added a `NormalizedPath` wrapper for working with logically or physically normalized paths
- Added builder-like methods to `EnvConfig` to facilitate replacing the type of any environment
implementation without having to specify all other values again
- Added `FileDescExt::into_evented2` which gracefully handles regular files (which cannot be
registered with tokio)
- Added a `spawn::function_body` method for spawning a function without having to look it up
from the environment

### Changed
- Reduced required bounds for implementing `VarEnvRestorer` to just `E: VariableEnvironment`
- Spawning a simple command now (more) correctly evaluates variable assignments where one
assignment depends on an earlier one (e.g. `var1=foo var2=${bar:-$var1} env`)
- Removed aggressive redirect restorer pre-allocations when evaluating redirects: most shell
scripts will not apply redirects to the majority of commands, nor will they have obscene
amounts of redirects when they do occur, so we can avoid allocating memory until we really need it.

### Deprecated
- Deprecated `eval_redirects_or_var_assignments`, `eval_redirects_or_cmd_words_with_restorer`
and `EvalRedirectOrVarAssig`: the existing implementation does not handle referencing earlier
variable assignments (e.g. `var1=foo var2=${bar:-$var1} env`) but cannot be ammended without
introducing breaking changes
- Deprecated `FileDescExt::into_evented` since it does not handle regular files gracefully

### Removed

### Fixed
- `EventedAsyncIoEnv` gracefully handles regular files by not registering them with tokio
(since epoll/kqueue do not support registering regular files)
- Fixed the behavior of an unset `$PATH` variable to behave like other shells (i.e. raises command
not found errors) instead of using the `PATH` env variable of the current process

### Security
### Breaking

## [0.1.1] - 2017-09-13
### Added
- Added `RedirectEnvRestorer` trait to abstract over `RedirectRestorer` and other implementations
- Added `spawn_with_local_redirections_and_restorer` to allow specifying a specific `RedirectEnvRestorer` implementation
- Added `VarEnvRestorer` trait to abstract over `VarRestorer` and other implementations
- Added `VirtualWorkingDirEnv::with_path_buf` constructor to avoid unneccessary `PathBuf` copies

### Changed
- `eval_redirects_or_cmd_words_with_restorer` is now generic over a `RedirectEnvRestorer`
- `EvalRedirectOrCmdWordError` is also generic over any `RedirectEnvRestorer`, but defaults to `RedirectRestorer` for backward compatibility
- `eval_redirects_or_var_assignments_with_restorer` is now generic over a `RedirectEnvRestorer`
- `EvalRedirectOrVarAssig` is also generic over any `RedirectEnvRestorer`, but defaults to `RedirectRestorer` for backward compatibility
- `LocalRedirections` is also generic over any `RedirectEnvRestorer`, but defaults to `RedirectRestorer` for backward compatibility

### Deprecated
- Deprecated most of the direct methods on `RedirectRestorer` in favor of the `RedirectEnvRestorer` trait
- Deprecated most of the direct methods on `VarRestorer` in favor of the `VarEnvRestorer` trait

## 0.1.0 - 2017-08-21
- First release!

[Unreleased]: https://github.com/ipetkov/conch-runtime/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/ipetkov/conch-runtime/compare/v0.1.0...v0.1.1
