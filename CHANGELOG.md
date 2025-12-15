# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2](https://github.com/sandhose/opentelemetry-instrumentation-tokio/compare/v0.1.1...v0.1.2) - 2025-12-15

### Fixed

- *(docs)* Fix docs.rs build
- *(perf)* Pre-compute the labels for the poll time histogram buckets
- *(perf)* Pre-compute per-worker set of labels

### Other

- Move the runtime registration logic to the runtime module

## [0.1.1](https://github.com/sandhose/opentelemetry-instrumentation-tokio/compare/v0.1.0...v0.1.1) - 2025-12-12

### Other

- Setup CI
