# Near File and Folder Descriptions

Near supports Far-style sidecar descriptions without making them part of universal resource identity. Providers expose descriptions through the reserved `near.description` metadata extension and optional mutation methods. Panel modes with a `description` column render that value; `Ctrl+Z` edits the selected resources or the current resource.

## Configuration

`descriptions.toml` is a layered schema-1 document:

- `description_files` is the ordered list of per-folder file-description catalogs. The first existing file is read and updated; otherwise the first name is created.
- `folder_description_files` is the ordered list used by F9 → View/Edit folder description. Editing creates the first configured name when none exists.
- `encoding` is `utf8`, `utf8-bom`, or `latin1`. An existing UTF-8 BOM always overrides the configured decoder. `utf8-bom` writes a BOM; Latin-1 writes fail visibly when text is not representable.
- `update_policy` is `always` or `disabled`.
- `show_description_files` controls whether file-description catalogs appear as normal panel entries.

CLI and environment overrides are `--descriptions` and `NEAR_DESCRIPTIONS`.

## Catalog Format

Each non-comment line contains a filename followed by free-form description text. Names containing whitespace or quotes are double quoted; embedded quotes use `\"`. Blank lines and lines beginning with `#` or `;` are ignored. Catalog writes are sorted and replaced through a temporary file.

## Operation Semantics

When updates are enabled, successful top-level local operations keep catalogs synchronized:

- copy duplicates the source description at the actual destination name;
- move and rename transfer the description and remove the source entry;
- trash and permanent delete remove the source entry;
- recursive descendants are not rewritten implicitly.

Description-update failure is reported as an item failure after the filesystem mutation, preserving an explicit partial-operation diagnostic rather than silently losing catalog state.
