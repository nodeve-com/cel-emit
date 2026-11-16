# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
