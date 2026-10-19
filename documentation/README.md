# Mac Cleaner documentation

Documentation for the implemented **0.1.1** application: a Rust/Tauri macOS cleaner with a TypeScript interface. Checked against the source on 19 October 2026.

Start with the user guide to scan and review files, or the architecture guide to work on the code.

| Guide | Contents |
| --- | --- |
| [Features and user guide](features.md) | Every screen, supported scan location, selection, uninstall, Trash, restore, and settings |
| [Architecture and file map](architecture.md) | Application layers, execution flow, dependencies, and the purpose of every source/configuration file |
| [Rust backend and native bridge](backend.md) | Discovery, measurement, validation, cleanup, history reconciliation, restoration, and macOS integration |
| [Frontend code](frontend.md) | State, rendering functions, event handlers, styles, themes, and accessibility |
| [IPC and data reference](api.md) | All eleven commands, scan events, models, persistence, and history statuses |
| [Development and testing](development.md) | Setup, scripts, builds, test coverage, packaging, and where to make changes |
| [Safety, limitations, and troubleshooting](safety.md) | What the safeguards enforce, remaining risks, recovery failures, and release limitations |

## Implementation status

Scanning, explicit selection, review, native Trash movement, app-bundle uninstall, persistent history, and restoration are implemented. The local Apple Silicon build and automated checks are recorded in [VALIDATION.md](../VALIDATION.md). This documentation does not turn unverified behavior into a release guarantee.

The ten-second scan duration is a target. The UI reports measured elapsed time. Trash preserves the possibility of recovery while the item remains accessible; moving an item to Trash does not immediately reclaim its disk space.

## Related project material

- [Project README](../README.md): quick start and common commands.
- [PLAN.md](../PLAN.md): original plan and implementation status; some sections describe intended work.
- [DESIGN.md](../DESIGN.md): visual direction and design decisions.
- [Reference image](../example-image.png): the supplied interface reference.
- [VALIDATION.md](../VALIDATION.md): dated checks and remaining release work.

These guides describe current source behavior. When changing a feature, update its guide, the [IPC reference](api.md) if its contract changes, and the validation record only after running the relevant checks.
