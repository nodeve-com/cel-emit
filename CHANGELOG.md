# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
