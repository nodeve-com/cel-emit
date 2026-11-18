# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.0](https://github.com/nodeve-com/cel-emit/compare/cel-emit-schema-bridge-v0.10.1...cel-emit-schema-bridge-v0.11.0) - 2026-11-18

### Added

- [**breaking**] a dynamic value failing to fill a message names the place inside it

### Other

- a failure inside a decoded value names its place

## [0.10.1](https://github.com/nodeve-com/cel-emit/compare/cel-emit-schema-bridge-v0.10.0...cel-emit-schema-bridge-v0.10.1) - 2026-11-18

### Other

- updated the following local packages: cel-emit

## [0.10.0](https://github.com/nodeve-com/cel-emit/compare/cel-emit-schema-bridge-v0.9.0...cel-emit-schema-bridge-v0.10.0) - 2026-11-18

### Fixed

- *(bridge)* [**breaking**] a property neither required nor defaulted reads as an optional

## [0.9.0](https://github.com/nodeve-com/cel-emit/compare/cel-emit-schema-bridge-v0.8.0...cel-emit-schema-bridge-v0.9.0) - 2026-11-16

### Added

- *(bridge)* take schema-emit 0.7.2; a tuple is an opaque type wherever it sits, read and written by functions made of schema-emit's own

### Other

- *(bridge)* a tuple, moved whole, and its functions

## [0.8.0](https://github.com/nodeve-com/cel-emit/compare/cel-emit-schema-bridge-v0.7.1...cel-emit-schema-bridge-v0.8.0) - 2026-11-16

### Added

- *(bridge)* take schema-emit 0.7.1; a record filling a dynamic reference with a type that is no object reads and writes by its own functions
- a type argument names any type declared under a name

### Fixed

- *(bridge)* a record another module holds is named by that module's path

### Other

- each function within clippy's line count

## [0.7.1](https://github.com/nodeve-com/cel-emit/compare/cel-emit-schema-bridge-v0.7.0...cel-emit-schema-bridge-v0.7.1) - 2026-11-16

### Fixed

- *(bridge)* a field the standard form fills later types as one that may be empty

## [0.7.0](https://github.com/nodeve-com/cel-emit/compare/cel-emit-schema-bridge-v0.6.2...cel-emit-schema-bridge-v0.7.0) - 2026-11-15

### Other

- take schema-emit 0.7.0; a program links schema-emit-runtime at its own version
- the bridge's named types are the definitions a schema names
- The bridge reads schema-emit's filled schema and the caller's standard form
- the bridge reads schema-emit's result, never a schema document
- Call a schema's open part a dynamic reference
