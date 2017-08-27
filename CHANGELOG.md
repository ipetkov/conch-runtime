# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- `Env` will now manage `$PWD` and `$OLDPWD` whenever the current working directory is changed

### Changed
### Deprecated
### Removed
### Fixed
### Security
### Breaking
- Instantiating an `Env` now requires its `WD` parameter to implement `WorkingDirectoryEnvironment`
for managing the `$PWD` and `$OLDPWD` environment variables
- The `WorkingDirectoryEnvironment` implementation of `Env` now requires that it also implements
`VariableEnvironment` for managing the `$PWD` and `$OLDPWD` environment variables

## [0.1.0] - 2017-08-21
- First release!

[Unreleased]: https://github.com/ipetkov/conch-runtime/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/ipetkov/conch-runtime/compare/v0.1.0...HEAD
