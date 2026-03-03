/**
 * vscode-hief — VS Code extension for HIEF (Hybrid Intent-Evaluation Framework)
 *
 * Provides visual intent management, dependency graph viewing,
 * evaluation score display, and one-click approve/reject workflows.
 */

import * as vscode from "vscode";
import { HiefProjectManager } from "./backend/HiefProjectManager";
import { IntentBoardProvider } from "./providers/IntentBoardProvider";
import { DashboardProvider } from "./providers/DashboardProvider";

let projectManager: HiefProjectManager;

export function activate(context: vscode.ExtensionContext) {
  const workspaceRoot =
    vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? "";
  projectManager = new HiefProjectManager(workspaceRoot);

  // Register webview providers
  const intentProvider = new IntentBoardProvider(
    context.extensionUri,
    projectManager
  );
  const dashboardProvider = new DashboardProvider(
    context.extensionUri,
    projectManager
  );

  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(
      "hief.intentsPanel",
      intentProvider
    ),
    vscode.window.registerWebviewViewProvider(
      "hief.dashboardPanel",
      dashboardProvider
    )
  );

  // Register commands
  context.subscriptions.push(
    vscode.commands.registerCommand("hief.showKanban", () => {
      intentProvider.showKanbanPanel(context);
    }),

    vscode.commands.registerCommand("hief.showIntents", () => {
      intentProvider.showIntentListPanel(context);
    }),

    vscode.commands.registerCommand("hief.showDashboard", () => {
      dashboardProvider.showDashboardPanel(context);
    }),

    vscode.commands.registerCommand("hief.showGraph", () => {
      showDependencyGraph(context);
    }),

    vscode.commands.registerCommand("hief.searchCode", async () => {
      const query = await vscode.window.showInputBox({
        prompt: "Search code index",
        placeHolder: "function name, class, pattern...",
      });
      if (query) {
        const results = await projectManager.searchCode(query);
        showSearchResults(results);
      }
    }),

    vscode.commands.registerCommand("hief.runDoctor", async () => {
      const report = await projectManager.runDoctor();
      vscode.window.showInformationMessage(
        `HIEF Doctor: ${report.healthy ? "✅ Healthy" : "❌ Issues detected"} (${report.checks.length} checks)`
      );
    }),

    vscode.commands.registerCommand("hief.runEval", async () => {
      const results = await projectManager.runEval();
      if (results.length > 0) {
        const first = results[0];
        vscode.window.showInformationMessage(
          `Eval '${first.golden_set}': ${first.passed ? "✅ PASS" : "❌ FAIL"} (score: ${first.overall_score.toFixed(2)})`
        );
      }
    }),

    vscode.commands.registerCommand("hief.approveIntent", async () => {
      const id = await vscode.window.showInputBox({
        prompt: "Enter intent ID to approve",
        placeHolder: "hief-a1b2 or a1b2",
      });
      if (id) {
        await projectManager.updateIntent(id, "approved");
        vscode.window.showInformationMessage(`✅ Intent ${id} approved`);
        intentProvider.refresh();
      }
    }),

    vscode.commands.registerCommand("hief.rejectIntent", async () => {
      const id = await vscode.window.showInputBox({
        prompt: "Enter intent ID to reject",
        placeHolder: "hief-a1b2 or a1b2",
      });
      if (id) {
        await projectManager.updateIntent(id, "rejected");
        vscode.window.showInformationMessage(`❌ Intent ${id} rejected`);
        intentProvider.refresh();
      }
    }),

    vscode.commands.registerCommand("hief.createIntent", async () => {
      const kind = await vscode.window.showQuickPick(
        ["feature", "bug", "refactor", "spike", "test", "chore"],
        { placeHolder: "Intent kind" }
      );
      if (!kind) return;

      const title = await vscode.window.showInputBox({
        prompt: "Intent title",
        placeHolder: "Short description of the work",
      });
      if (!title) return;

      const priority = await vscode.window.showQuickPick(
        ["critical", "high", "medium", "low"],
        { placeHolder: "Priority" }
      );

      const intent = await projectManager.createIntent(
        kind,
        title,
        priority ?? "medium"
      );
      vscode.window.showInformationMessage(
        `✅ Created intent: ${intent.id} (${intent.title})`
      );
      intentProvider.refresh();
    }),

    vscode.commands.registerCommand("hief.refresh", () => {
      intentProvider.refresh();
      dashboardProvider.refresh();
    })
  );

  // Status bar item
  const statusBar = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    100
  );
  statusBar.text = "$(beaker) HIEF";
  statusBar.tooltip = "HIEF — Hybrid Intent-Evaluation Framework";
  statusBar.command = "hief.showDashboard";
  statusBar.show();
  context.subscriptions.push(statusBar);

  console.log("vscode-hief activated");
}

export function deactivate() {
  console.log("vscode-hief deactivated");
}

// ---------------------------------------------------------------------------
// Helper panels
// ---------------------------------------------------------------------------

function showDependencyGraph(context: vscode.ExtensionContext) {
  const panel = vscode.window.createWebviewPanel(
    "hief.graph",
    "HIEF Dependency Graph",
    vscode.ViewColumn.One,
    { enableScripts: true }
  );
  panel.webview.html = `<!DOCTYPE html>
<html><head><style>
  body { font-family: var(--vscode-font-family); background: var(--vscode-editor-background); color: var(--vscode-editor-foreground); padding: 20px; }
  h2 { margin-bottom: 10px; }
  .placeholder { text-align: center; padding: 60px; opacity: 0.6; }
</style></head><body>
  <div class="placeholder">
    <h2>🔗 Dependency Graph</h2>
    <p>Interactive DAG visualization coming in Phase 3.</p>
    <p>Run <code>hief graph validate</code> to check graph integrity.</p>
  </div>
</body></html>`;
}

function showSearchResults(results: any[]) {
  const panel = vscode.window.createWebviewPanel(
    "hief.search",
    "HIEF Code Search",
    vscode.ViewColumn.One,
    { enableScripts: true }
  );

  const rows = results
    .map(
      (r: any, i: number) =>
        `<tr>
      <td>${i + 1}</td>
      <td>${r.symbol_name ?? "(anonymous)"}</td>
      <td>${r.symbol_kind ?? ""}</td>
      <td>${r.file_path}:${r.start_line}–${r.end_line}</td>
      <td><pre>${escapeHtml(r.content?.substring(0, 200) ?? "")}</pre></td>
    </tr>`
    )
    .join("");

  panel.webview.html = `<!DOCTYPE html>
<html><head><style>
  body { font-family: var(--vscode-font-family); background: var(--vscode-editor-background); color: var(--vscode-editor-foreground); padding: 20px; }
  table { width: 100%; border-collapse: collapse; }
  th, td { text-align: left; padding: 6px 10px; border-bottom: 1px solid var(--vscode-panel-border); }
  pre { margin: 0; font-size: 12px; white-space: pre-wrap; }
</style></head><body>
  <h2>🔍 Search Results (${results.length})</h2>
  <table>
    <tr><th>#</th><th>Symbol</th><th>Kind</th><th>Location</th><th>Preview</th></tr>
    ${rows}
  </table>
</body></html>`;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
