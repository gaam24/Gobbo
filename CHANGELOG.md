# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses semantic versions.

## Unreleased

### Fixed

- A fresh clone failed to build the backend because three Rust modules were excluded from the repository by the
  over-broad `target*` ignore pattern; the pattern is now anchored to the Cargo build directories.

## v0.1.1
