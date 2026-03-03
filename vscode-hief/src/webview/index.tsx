/**
 * React webview entry point for HIEF VS Code extension.
 *
 * This is the root component that renders the appropriate view
 * based on the view type passed from the extension host.
 *
 * Phase 1: Basic intent list and detail view
 * Phase 2: Kanban board with drag-and-drop
 * Phase 3: Dependency graph (dagre/d3)
 * Phase 4: Code search + eval scores
 * Phase 5: Dashboard + agent provenance
 */

import React from "react";
import { createRoot } from "react-dom/client";

interface AppProps {
  viewType: "kanban" | "list" | "dashboard" | "graph" | "search";
}

function App({ viewType }: AppProps) {
  return (
    <div className="hief-app">
      <h2>HIEF — {viewType}</h2>
      <p>
        React webview scaffolded. Full implementation coming in subsequent
        phases.
      </p>
    </div>
  );
}

// Mount
const container = document.getElementById("root");
if (container) {
  const root = createRoot(container);
  root.render(<App viewType="kanban" />);
}
