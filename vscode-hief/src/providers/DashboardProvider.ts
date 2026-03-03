/**
 * Dashboard webview provider — shows project health overview.
 *
 * Displays index stats, intent counts by status, eval scores,
 * doctor check results, and hook status.
 */

import * as vscode from "vscode";
import { HiefProjectManager } from "../backend/HiefProjectManager";

export class DashboardProvider implements vscode.WebviewViewProvider {
  private _view?: vscode.WebviewView;

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly projectManager: HiefProjectManager
  ) {}

  resolveWebviewView(
    webviewView: vscode.WebviewView,
    _context: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken
  ) {
    this._view = webviewView;
    webviewView.webview.options = { enableScripts: true };
    this.updateContent();
  }

  refresh() {
    this.updateContent();
  }

  showDashboardPanel(context: vscode.ExtensionContext) {
    const panel = vscode.window.createWebviewPanel(
      "hief.dashboardFull",
      "HIEF Dashboard",
      vscode.ViewColumn.One,
      { enableScripts: true }
    );

    this.buildDashboardHtml().then((html) => {
      panel.webview.html = html;
    });
  }

  private async updateContent() {
    if (!this._view) return;
    this._view.webview.html = await this.buildDashboardHtml();
  }

  private async buildDashboardHtml(): Promise<string> {
    let intentSummary = { total: 0, byStatus: {} as Record<string, number> };
    let indexInfo = { files: 0, chunks: 0, dbSize: "0" };
    let doctorHealthy = true;

    try {
      const intents = await this.projectManager.listIntents();
      intentSummary.total = intents.length;
      for (const i of intents) {
        intentSummary.byStatus[i.status] =
          (intentSummary.byStatus[i.status] || 0) + 1;
      }
    } catch {
      /* no data */
    }

    try {
      const stats = await this.projectManager.indexStatus();
      indexInfo.files = stats.total_files;
      indexInfo.chunks = stats.total_chunks;
      indexInfo.dbSize = formatBytes(stats.db_size_bytes);
    } catch {
      /* no data */
    }

    try {
      const report = await this.projectManager.runDoctor();
      doctorHealthy = report.healthy;
    } catch {
      /* no data */
    }

    const statusRows = Object.entries(intentSummary.byStatus)
      .map(
        ([status, count]) =>
          `<div class="stat-row"><span>${statusIcon(status)} ${status}</span><span class="stat-value">${count}</span></div>`
      )
      .join("");

    return `<!DOCTYPE html>
<html><head><style>
  body { font-family: var(--vscode-font-family); padding: 12px; margin: 0; }
  .section { margin-bottom: 16px; }
  .section h3 { margin: 0 0 8px; font-size: 13px; border-bottom: 1px solid var(--vscode-panel-border); padding-bottom: 4px; }
  .stat-row { display: flex; justify-content: space-between; padding: 3px 0; font-size: 12px; }
  .stat-value { font-weight: 600; font-family: monospace; }
  .health { padding: 8px; border-radius: 4px; text-align: center; font-weight: 600; }
  .health-ok { background: #22c55e22; color: #22c55e; }
  .health-bad { background: #ef444422; color: #ef4444; }
</style></head><body>
  <div class="section">
    <div class="health ${doctorHealthy ? "health-ok" : "health-bad"}">
      ${doctorHealthy ? "✅ Project Healthy" : "❌ Issues Detected — run hief doctor"}
    </div>
  </div>

  <div class="section">
    <h3>📋 Intents (${intentSummary.total})</h3>
    ${statusRows || '<div class="stat-row" style="opacity:0.5">No intents</div>'}
  </div>

  <div class="section">
    <h3>📊 Code Index</h3>
    <div class="stat-row"><span>Files</span><span class="stat-value">${indexInfo.files}</span></div>
    <div class="stat-row"><span>Chunks</span><span class="stat-value">${indexInfo.chunks}</span></div>
    <div class="stat-row"><span>DB Size</span><span class="stat-value">${indexInfo.dbSize}</span></div>
  </div>
</body></html>`;
  }
}

function statusIcon(status: string): string {
  const icons: Record<string, string> = {
    draft: "📝",
    approved: "✅",
    in_progress: "🔨",
    in_review: "👀",
    verified: "✔️",
    merged: "🎉",
    rejected: "❌",
    blocked: "🔒",
  };
  return icons[status] || "❓";
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
