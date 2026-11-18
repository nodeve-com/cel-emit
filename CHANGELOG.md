# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.1](https://github.com/nodeve-com/cel-emit/compare/cel-emit-v0.8.0...cel-emit-v0.8.1) - 2026-11-18

### Fixed

- an embedder's Rust for a built-in overload takes its arguments one each

### Other

- *(cel-emit-schema-bridge)* release v0.10.0 ([#5](https://github.com/nodeve-com/cel-emit/pull/5))

## [0.8.0](https://github.com/nodeve-com/cel-emit/compare/cel-emit-v0.7.0...cel-emit-v0.8.0) - 2026-11-16

### Added

- *(bridge)* take schema-emit 0.7.2; a tuple is an opaque type wherever it sits, read and written by functions made of schema-emit's own
- an opaque type, a Rust type the embedder names, which an expression moves and never reads

### Other

- an opaque value, moved whole; a decoded list of them a gap

## [0.7.0](https://github.com/nodeve-com/cel-emit/compare/cel-emit-v0.6.3...cel-emit-v0.7.0) - 2026-11-16

### Added

- *(bridge)* take schema-emit 0.7.1; a record filling a dynamic reference with a type that is no object reads and writes by its own functions
- a type argument names any type declared under a name

### Fixed

- string(bool) gives true or false

### Other

- *(cel-emit-schema-bridge)* release v0.7.1 ([#2](https://github.com/nodeve-com/cel-emit/pull/2))

## [0.6.3](https://github.com/nodeve-com/cel-emit/compare/cel-emit-v0.6.2...cel-emit-v0.6.3) - 2026-11-15

### Other

- take schema-emit 0.7.0; a program links schema-emit-runtime at its own version
