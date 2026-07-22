# Near Profiles

Near separates configuration from mutable application state. `--config-root DIR` redirects layered user configuration, while `--data-root DIR` redirects histories, positions, the operation journal, plugin packages, and plugin grants. `NEAR_CONFIG_HOME` and `NEAR_DATA_HOME` provide the equivalent environment-level controls.

`near-fm --portable DIR` uses `DIR/config` and `DIR/state` for both roots. Relative paths are resolved from the startup directory, making the resulting profile self-contained and relocatable without changing the process environment.

`near-fm --export-profile BUNDLE` writes a versioned, inspectable directory containing `near-profile.toml`, `config/`, and `state/`. Only known configuration and state filenames are exported. Existing destinations are rejected so an earlier backup cannot be overwritten accidentally.

`near-fm --import-profile BUNDLE` validates the schema, application identity, and every listed filename before creating destination roots. Files outside the whitelist and traversal paths are rejected. Each imported file is copied to a temporary sibling, the previous file is backed up, and replacement is renamed into place with rollback on failure. Import and export are standalone startup actions and never initialize the interactive terminal.
