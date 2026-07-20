# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).



## [0.2.0](https://github.com/rvben/shelly-cli/compare/v0.1.12...v0.2.0) - 2026-07-20

### Breaking Changes

- **core**: typed shelly-core::Error + host-string addressing ([2a5af3f](https://github.com/rvben/shelly-cli/commit/2a5af3ff27e06e8ffa388fc4d6a3dc3aba01f8b2))

### Added

- **core**: implement switchkit::SmartDevice for Shelly ([0dd38cc](https://github.com/rvben/shelly-cli/commit/0dd38cc13eac1737d38f2b37e00536ef27629ba3))
- **core**: typed shelly-core::Error + host-string addressing ([2a5af3f](https://github.com/rvben/shelly-cli/commit/2a5af3ff27e06e8ffa388fc4d6a3dc3aba01f8b2))

### Fixed

- **core**: firmware_version honors the unknown sentinel; Error non_exhaustive ([6805f74](https://github.com/rvben/shelly-cli/commit/6805f745238968f106c7789c87126c625ecaaab6))
- **core**: don't fabricate 'unknown' metadata or misclassify hostname Shellys ([3899482](https://github.com/rvben/shelly-cli/commit/38994822b534fef7736b8647d05e395df83076a8))

## [0.1.12](https://github.com/rvben/shelly-cli/compare/v0.1.11...v0.1.12) - 2026-06-11

### Added

- make shelly CLI fully compliant with The CLI Spec v0.2 ([478bec8](https://github.com/rvben/shelly-cli/commit/478bec8953be26bd914dce0b4c70165cd0329a7b))

## [0.1.11](https://github.com/rvben/shelly-cli/compare/v0.1.10...v0.1.11) - 2026-05-24

### Added

- **light**: mark light mutating commands in schema ([d207bc4](https://github.com/rvben/shelly-cli/commit/d207bc4122883a40c3459a2a90ff194ded98ac0a))
- **light**: add shelly light command for Gen2/Gen3 RGB control ([aa5d49b](https://github.com/rvben/shelly-cli/commit/aa5d49ba6bc70a49acac70d65b4f135efe6a4958))
