# Far Manager 3 Startup, Profiles, and Environment

This appendix reflects the English help in official build `3.0.6703.0`.

## Command-line syntax

Far accepts switches followed by up to two locations or plugin-prefix commands. The first target opens in the active panel and the second in the passive panel. A file target opens its containing folder and places the cursor on the file; a folder or archive target opens directly.

| Switch | Purpose |
|---|---|
| `-e[<line>[:<pos>]] <file>` | Edit a file, optionally at line and character position. |
| `-v <file>` | View a file. Use `-` to read standard input, e.g. `dir | far -v -`. |
| `-p[<paths>]` | Set semicolon-separated main plugin search paths. Empty `-p` disables plugins. |
| `-co` | Load plugins only from cache. Ignored when `-p` is specified. |
| `-m` | Do not load macros. |
| `-ma` | Load macros but do not run macros marked “Run after Far start.” |
| `-s <profile> [<local-profile>]` | Override roaming and optional local profile locations. |
| `-u <username>` | Set legacy per-user identity for Far 1.x plugins and `FARUSER`. |
| `-w` / `-w-` | Select console-window versus console-buffer interface mode. |
| `-t <template-profile>` | Override the template configuration location. |
| `-title[:<title>]` | Use a custom title or inherit the console title. `%Default` expands to Far’s normal contextual title. |
| `-clearcache [profile [local-profile]]` | Clear plugin cache. |
| `-export <file.farconfig> [profile [local-profile]]` | Export configuration. |
| `-import <file.farconfig> [profile [local-profile]]` | Import configuration. |
| `-ro` / `-ro-` | Force read-only or normal configuration mode. |
| `-set:<parameter>=<value>` | Override a `far:config` parameter for this launch. |
| `-x` | Disable exception handling for development/debugging. |

Example targets:

```text
far C:\Work D:\Output
far -e70:2 README.md
dir | far -v -
far arc:C:\Packages\sample.7z "lua:msgbox('Far Manager','Started')"
```

When two plugin-prefix commands are supplied, the passive command is executed first while the passive panel is temporarily active. Single-letter prefixes that conflict with drive letters are ignored.

## Plugin loading rules

Without `-p` or `-co`, Far discovers plugins in:

1. The `Plugins` directory beside `Far.exe`.
2. The `Plugins` directory under the user profile, normally `%APPDATA%\Far Manager\Profile\Plugins`.

Rules:

- `-p` with paths limits discovery to those paths.
- Empty `-p` starts Far without plugins.
- `-co` uses only the existing plugin cache, making startup faster but ignoring new, changed, or removed plugins.
- Start once without `-co` after modifying the plugin set.

## Profile model

Far distinguishes roaming and local state:

- **Roaming profile**: settings and user data intended to follow a user, including additional plugins and macro configuration.
- **Local profile**: histories, caches, and other machine-local data.
- **Template profile**: initial/default configuration used to seed or constrain setup.

Use `-s` for portable, isolated, test, or side-by-side profiles. `-export` and `-import` provide a supported configuration migration format. `-ro` protects a profile from writes for testing or controlled deployments.

## Environment variables set for child processes

| Variable | Value |
|---|---|
| `FARHOME` | Directory containing the main Far executable. |
| `FARPROFILE` | Roaming user-data directory. |
| `FARLOCALPROFILE` | Local user-data directory. |
| `FARLANG` | Current interface language name. |
| `FARUSER` | Username supplied through `-u`. |
| `FARDIRSTACK` | Top of the directory stack managed by `pushd` and `popd`. |
| `FARADMINMODE` | `1` when Far runs under an administrator account. |

These variables are available to commands, scripts, file associations, user-menu entries, and tools launched from Far.

## Configuration hierarchy

1. Normal Options dialogs expose supported everyday settings.
2. `far:config` exposes advanced and compatibility settings.
3. `-set:<name>=<value>` temporarily overrides a configuration parameter.
4. Plugins maintain their own settings and configuration dialogs.
5. LuaMacro scripts can dynamically change behavior or replace hotkeys.

For reproducible troubleshooting, test with an isolated `-s` profile and optionally `-m` or empty `-p` to separate core Far behavior from macros and plugins.

## Practical launch profiles

### Clean core-only session

```text
far -m -p
```

Starts without macros or plugins. This is useful for diagnosing whether an extension changes a key or operation.

### Isolated test profile

```text
far -s C:\Temp\FarProfile C:\Temp\FarLocal
```

Keeps test settings and histories away from the normal profile.

### Read-only inspection

```text
far -ro -s C:\Controlled\FarProfile C:\Temp\FarLocal
```

Uses a protected roaming profile while allowing a separate local area when policy permits.

### Open two working locations

```text
far C:\Source D:\Destination
```

Immediately creates the canonical source/destination two-panel workspace.

