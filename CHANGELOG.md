# Changelog

## [Unreleased]

### Added

- Add opt-in `show_hidden` indexing for hidden files/directories in non-git roots; git roots already include hidden non-ignored files, existing ignore rules still apply, and `.git` internals remain excluded.

### Fixed

- Preserve pre-feature picker rendering when ordered fuzzy parts are enabled.
- Enforce strict typed order for fuzzy chunks despite typo tolerance.
