# Third-party notices

Kairo's independent first-party source retains MIT, including its copyright/permission notice in LICENSE. This project does not relicense dependencies or historical reference material.

Cargo.lock pins versions. [DEPENDENCIES.tsv](docs/DEPENDENCIES.tsv) identifies declared package terms; [THIRD_PARTY_LICENSES.txt](docs/THIRD_PARTY_LICENSES.txt) retains discovered top-level and nested notices, including ring's embedded once_cell. [Dependency selections](docs/DEPENDENCY_LICENSE_DECISIONS.tsv) records compatible branches; AND conditions remain cumulative. Unicode-3.0 and CDLA-Permissive-2.0 notices remain effective. An optional LGPL branch is not a mandatory selection.

Rust standard-library copyright metadata and license/exception texts remain under licenses/rust-stdlib. winapi leaf-package MIT notices remain under licenses/winapi-MIT.txt. The complete Cargo inventory includes build/dev/cross-target packages and is not proof every item is linked into every binary. Dynamically linked system libraries are supplied separately by the operating system; bundling or statically linking them requires a fresh license review.

The original public Gist had no explicit license at the pinned revision. No original script is included, and no permission over it is claimed. The independent Rust comparison and limits are described in [provenance](docs/PROVENANCE.md). Historical private Python code and its uncertain rights chain are excluded.

MIT remains the project license because it matches the existing grant. Apache-2.0 is technically compatible with the selected dependency terms, but switching project licensing requires an owner decision and valid licensing authority; simply replacing LICENSE would not establish that authority.
