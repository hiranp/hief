# vscode-hief

VS Code extension for [HIEF](https://github.com/hiranp/hief) (Hybrid Intent-Evaluation Framework).

## Features

- **Intent Kanban Board** — Drag-and-drop cards between status columns (Draft → Approved → In Progress → In Review → Verified → Merged)
- **Visual Approvals** — One-click approve/reject intents directly from the board or sidebar
- **Intent List** — Sortable, filterable table of all intents with status, priority, and assignment
- **Dashboard** — Project health overview: index stats, intent counts, eval scores, doctor status
- **Dependency Graph** — Interactive DAG visualization of intent dependencies *(Phase 3)*
- **Code Search** — AST-aware code search panel powered by HIEF's FTS5 index *(Phase 4)*
- **Eval Score Display** — Golden-set pass/fail rates and quality scores inline *(Phase 4)*

## Prerequisites

- [HIEF CLI](https://github.com/hiranp/hief) installed and available on PATH (`hief` command)
- A project initialized with `hief init`

## Installation

```sh
# From the vscode-hief directory
npm install
npm run build
```

Then install the extension:
- Press `F1` → "Extensions: Install from VSIX"
- Or run: `code --install-extension vscode-hief-0.1.0.vsix`

## Usage

1. Open a project that has been initialized with `hief init`
2. The HIEF icon appears in the Activity Bar (left sidebar)
3. Click it to see the Intents panel and Dashboard

### Commands

| Command | Description |
|---|---|
| `HIEF: Show Intent Kanban Board` | Full Kanban board with status columns |
| `HIEF: Show Intent List` | Detailed sortable table |
| `HIEF: Show Dashboard` | Project health overview |
| `HIEF: Show Dependency Graph` | Intent dependency DAG |
| `HIEF: Search Code Index` | Search indexed code |
| `HIEF: Run Doctor` | Run health checks |
| `HIEF: Run Evaluation` | Run golden-set evaluation |
| `HIEF: Approve Intent` | Approve an intent (status → approved/verified) |
| `HIEF: Reject Intent` | Reject an intent |
| `HIEF: Create Intent` | Create a new intent interactively |

## Architecture

```
vscode-hief/
├── src/
│   ├── extension.ts              # Extension entry point
│   ├── backend/
│   │   ├── HiefProjectManager.ts # Wraps `hief --json` CLI calls
│   │   └── types.ts              # TypeScript types mirroring Rust structs
│   ├── providers/
│   │   ├── IntentBoardProvider.ts # Kanban + list webview provider
│   │   └── DashboardProvider.ts  # Dashboard webview provider
│   └── webview/
│       └── index.tsx             # React root (for future phases)
├── media/
│   └── hief-icon.svg             # Activity bar icon
├── package.json                  # Extension manifest
└── tsconfig.json
```

## Development Roadmap

| Phase | Scope | Status |
|---|---|---|
| **Phase 1** | Intent list + detail view + approval buttons | ✅ Scaffolded |
| **Phase 2** | Kanban board with drag-and-drop | ✅ Scaffolded |
| **Phase 3** | Dependency graph visualization (dagre/d3) | 📋 Planned |
| **Phase 4** | Code search + eval score integration | 📋 Planned |
| **Phase 5** | Dashboard + agent provenance timeline | 📋 Planned |

## License

MIT OR Apache-2.0
