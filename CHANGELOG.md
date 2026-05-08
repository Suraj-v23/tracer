# Changelog

All notable changes to Tracer will be documented here.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning follows [Semantic Versioning](https://semver.org/).

---

## [0.1.0] - 2026-05-08

### Added
- Node graph view — files and folders as wired cards on an infinite canvas
- Size-based colour coding (red → yellow → green → grey)
- Inline folder expansion — right-click → *Open in this space*
- Multi-window support — double-click or *Open in new space*
- Collapse branch — right-click expanded folder → *Collapse*
- Drag nodes freely with edge-panning near viewport boundaries
- Create file, create folder, move item, delete item
- Back / forward navigation with Backspace / `]` shortcuts
- Arrow key navigation between nodes
- Search, sort (by size / name / type), filter
- File details sidebar with size, path, dates, readonly status
- Accurate APFS disk sizes via `blocks × 512`
- Rayon parallel filesystem scan with 30-second TTL cache
- Breadcrumb navigation
- GitHub Actions CI + automated macOS `.dmg` release (Apple Silicon + Intel)
