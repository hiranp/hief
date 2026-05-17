# UI Validation Artifacts

Use these artifacts to validate the Task Tracking UI when the dashboard is empty.

## Why Controls Might Be Missing

Block/Unblock controls render in the review panel for an existing intent route:

- /ui/tasks/{id}
- /ui/review/{id}

If there are no intents, the dashboard has no links into those pages.

## Artifact 1: Seed Script

Script: scripts/seed_ui_validation.sh

Purpose:

- Creates two example intents.
- Moves both intents to approved status.
- Prints direct URLs for validation.

Run:

```bash
bash scripts/seed_ui_validation.sh
```

## Artifact 2: Manual Validation Flow

1. Start UI server:

```bash
cargo run -- ui --port 3190
```

1. Open dashboard URL printed by the seed script.
1. Open the printed task-detail URL.
1. Confirm the review panel shows:

- Block button
- Unblock button
- Move to review button

1. Click Block and Unblock.
1. Confirm response messaging appears in the review panel.

## Expected Outcome

- Dashboard is no longer empty.
- Task detail page renders PAVL and telemetry sections.
- Review panel actions are visible and server-validated.
