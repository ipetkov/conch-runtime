# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- `Env` will now manage `$PWD` and `$OLDPWD` whenever the current working directory is changed
- Added `Read`/`Write` impls for `&EventedFileDesc`
- Added the `FileDescOpener` trait for abstracting over opening files/pipes through the environment
- Added the `FileDescManager` trait to encompass opening file descriptors, managing their permissions,
and performing async I/O over them
- Added `PlatformSpecificFileDescManagerEnv` environment and an atomic
counterpart as a successor to `PlatformSpecificAsyncIoEnv`
- Added inherent methods on `ThreadPoolAsyncIoEnv` for any `AsyncIoEnvironment`
related operations for more efficient operations which do not require owned
`FileDesc` handles as long as the provided input can be borrowed as a `FileDesc`
- Added the `ReportFailureEnvironment` trait for reporting arbitrary `Fail` types
- Added the `BuiltinEnvironment` and `BuiltinUtility` traits for spawning
builtin utilities
- Added `BuiltinEnv` as which is a `BuiltinEnvironment` implementation which
supports all provided builtin utilities definitions
- Added `simple_command_with_restorers` function which allows spawning a simple
command with the specified redirect and var restorers
- Added `FunctionFrameEnvironment` trait for tracking the stack size of
currently executing functions.

### Changed
- **Breaking:** Instantiating an `Env` now requires its `WD` parameter to implement `WorkingDirectoryEnvironment`
for managing the `$PWD` and `$OLDPWD` environment variables
- **Breaking:** The `WorkingDirectoryEnvironment` implementation of `Env` now requires that it also implements
`VariableEnvironment` for managing the `$PWD` and `$OLDPWD` environment variables
- **Breaking:** Corrected the signature of `VarRestorer::set_exported_var` to match that of `VarRestorer2::set_exported_var`
- **Breaking:** Corrected `EvalRedirectOrVarAssig` to handle earlier assignment references by using the implementation
of `EvalRedirectOrVarAssig2`
- **Breaking:** Bumped dependency of `winapi` to `0.3.4`
- **Breaking:** Improved debug printing of `spawn::Simple` after requiring a few additional
generic parameters to implement `Debug`
- **Breaking:** `AsyncIoEnvironment` must now specify an associated type `IoHandle` for what
file handle it accepts, thus it is no longer forced to operate with a `FileDesc`
- **Breaking:** `AsyncIoEnvironment` now returns an `io::Result` when creating an async read/write adapter
over a file handle, instead of surfacing the error at the first interaction with the new handle
- **Breaking:** `Env` now delegates file descriptor management to a single type instead of separate
file descriptor mapper, async I/O environment, etc.
- **Breaking:** `Env` can now delegate to a `BuiltinEnvironment` implementation
- **Breaking:** The `FileDescWrapper` trait now represents anything that can be
unwrapped into an owned `FileDesc` instead of anything that can be cloned and
borrowed as a `FileDesc`
- **Breaking:** EventedAsyncIoEnv has been rewritten to yield opaque file handles
- **Breaking:** Changing the blocking/nonblocking state of a `FileDesc` now
requires a mutable reference
- **Breaking:** `IsFatalError` now requires the implementor to also implement
`failure::Fail` instead of `std::error::Error`
- **Breaking:** All previous consumers of `ReportErrorEnvironment` now require
`ReportFailureEnvironment` implementations instead
- **Breaking:** All error types now implement `Fail` instead of `Error`.
Use `Fail::compat` to get back an `Error` implementation.
- **Breaking:** `ExecutableEnvironment` no longer enforces that
`Self::Future = CommandError` which gives implementors the flexibility to use
their own error types.
- **Breaking:** Spawning a simple command now requires that errors arising from
executing commands implement `Fail`
- **Breaking:** `ExecutorEnvironment::Future` has been renamed to `ExecFuture`
- **Breaking:** `ExecutorEnvironment` is now implemented for all `&mut T`
where `T: ExecutableEnvironment`
- **Breaking:** `SimpleCommand` now supports spawning builtin utilities, (but
requires that the environment support `Spawn`able builtins)
- **Breaking:** `Sequence` now requires the environment to implement
`IsInteractiveEnvironment` to avoid situations where it blocks waiting for
input which is not yet available
- **Breaking:** `Function` now keeps track of the current function stack size
and now requires that the environment implement `FunctionFrameEnvironment`
 - Subsequently, other combinators such as `Subshell`, `If`, `Case`,
`Substitution` and `ParameterSubstitution` also require the additional bound
for `IsInteractiveEnvironment`
- **Breaking:** the `shift` builtin command's spawned `Future` has been changed
to potentially write an error message, and is no longer just a simple `ExitStatus`.
- `SimpleCommand` is now generic over the redirect and var restorers it is
given. These generic parameters will default to `RedirectRestorer` and
`VarRestorer` to remain backwards compatible (which was effectively the
previous behavior as well).
- `RuntimeError` now implements `From<void::Void>` to satisfy type conversions
- Builtin commands now print out their error messages as part of their execution instead
of requiring the environment to report it

### Deprecated
- Deprecated `FileDescExt::into_evented2`, renamed to `FileDescExt::into_evented`
- Deprecated `VarRestorer2` since `VarRestorer` has been corrected and the two traits now behave
identically
- Deprecated `EvalRedirectOrVarAssig2` since it is now an alias for `EvalRedirectOrVarAssig`

### Removed
- **Breaking:** Removed the previous version of `FileDescExt::into_evented` and replaced it with
the signature of `FileDescExt::into_evented2`
- **Breaking:** Removed deprecated `RedirectEnvRestorer` methods
- **Breaking:** Removed deprecated `VarRestorer` methods
- **Breaking:** Removed deprecated `eval_redirects_or_var_assignments` and
`eval_redirects_or_var_assignments_with_restorer` functions
- **Breaking:** Removed `PlatformSpecificAsyncIoEnv`,
`PlatformSpecificRead`, and `PlatformSpecificWriteAll` as they are superceded
by the new `PlatformSpecificFileDescManagerEnv`
- **Breaking:** Removed the `ReportErrorEnvironment` trait, as it is superceded
by the `ReportFailureEnvironment` trait

### Fixed
* `EventedFileDesc` no longer attempts to reregister a file descriptor into the
event loop if the original `register` call returns `ErrorKind::AlreadyExists`

## [0.1.4] - 2018-01-27
### Changed
* Removed usage of nightly features on Windows. Windows builds now work on stable!

## [0.1.3] - 2018-01-25
### Fixed
* Fixed nightly windows builds by updating the unstable feature `unique` to `ptr_internals`

## [0.1.2] - 2018-01-03
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

### Fixed
- `EventedAsyncIoEnv` gracefully handles regular files by not registering them with tokio
(since epoll/kqueue do not support registering regular files)
- Fixed the behavior of an unset `$PATH` variable to behave like other shells (i.e. raises command
not found errors) instead of using the `PATH` env variable of the current process

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

[Unreleased]: https://github.com/ipetkov/conch-runtime/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/ipetkov/conch-runtime/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/ipetkov/conch-runtime/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/ipetkov/conch-runtime/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ipetkov/conch-runtime/compare/v0.1.0...v0.1.1
