/**
 * Webview provider for the Intent Board (sidebar panel + full Kanban).
 *
 * The sidebar shows a compact intent list.
 * The full Kanban board is opened as a separate webview panel.
 */

import * as vscode from "vscode";
import { HiefProjectManager } from "../backend/HiefProjectManager";
import type { Intent } from "../backend/types";

export class IntentBoardProvider implements vscode.WebviewViewProvider {
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

    // Handle messages from the webview
    webviewView.webview.onDidReceiveMessage(async (msg) => {
      switch (msg.type) {
        case "approve":
          await this.projectManager.updateIntent(msg.id, "approved");
          this.refresh();
          break;
        case "reject":
          await this.projectManager.updateIntent(msg.id, "rejected");
          this.refresh();
          break;
        case "showDetails":
          vscode.commands.executeCommand("hief.showIntents");
          break;
        case "refresh":
          this.refresh();
          break;
      }
    });
  }

  refresh() {
    this.updateContent();
  }

  private async updateContent() {
    if (!this._view) return;

    let intents: Intent[] = [];
    try {
      intents = await this.projectManager.listIntents();
    } catch {
      // CLI not available or no HIEF project
    }

    this._view.webview.html = this.getIntentListHtml(intents);
  }

  /** Open full Kanban board in a separate panel. */
  showKanbanPanel(context: vscode.ExtensionContext) {
    const panel = vscode.window.createWebviewPanel(
      "hief.kanban",
      "HIEF Kanban Board",
      vscode.ViewColumn.One,
      { enableScripts: true, retainContextWhenHidden: true }
    );

    this.projectManager.listIntents().then((intents) => {
      panel.webview.html = this.getKanbanHtml(intents);

      panel.webview.onDidReceiveMessage(async (msg) => {
        if (msg.type === "updateStatus") {
          await this.projectManager.updateIntent(msg.id, msg.status);
          const updated = await this.projectManager.listIntents();
          panel.webview.html = this.getKanbanHtml(updated);
          this.refresh();
        }
      });
    });
  }

  /** Open intent list in a separate panel. */
  showIntentListPanel(context: vscode.ExtensionContext) {
    const panel = vscode.window.createWebviewPanel(
      "hief.intentList",
      "HIEF Intent List",
      vscode.ViewColumn.One,
      { enableScripts: true }
    );

    this.projectManager.listIntents().then((intents) => {
      panel.webview.html = this.getDetailedListHtml(intents);
    });
  }

  // -----------------------------------------------------------------------
  // HTML generation
  // -----------------------------------------------------------------------

  private getIntentListHtml(intents: Intent[]): string {
    const statusIcon: Record<string, string> = {
      draft: "📝",
      approved: "✅",
      in_progress: "🔨",
      in_review: "👀",
      verified: "✔️",
      merged: "🎉",
      rejected: "❌",
      blocked: "🔒",
    };

    const items = intents
      .map(
        (i) => `
      <div class="intent-item" data-id="${i.id}">
        <span class="status">${statusIcon[i.status] || "❓"}</span>
        <span class="title">${escapeHtml(i.title)}</span>
        <span class="id">${i.id}</span>
        ${
          i.status === "in_review"
            ? `<div class="actions">
            <button onclick="approve('${i.id}')" class="btn-approve" title="Approve">✅</button>
            <button onclick="reject('${i.id}')" class="btn-reject" title="Reject">❌</button>
          </div>`
            : ""
        }
      </div>`
      )
      .join("");

    return `<!DOCTYPE html>
<html><head><style>
  body { font-family: var(--vscode-font-family); font-size: var(--vscode-font-size); padding: 8px; margin: 0; }
  .intent-item { display: flex; align-items: center; gap: 6px; padding: 6px 4px; border-bottom: 1px solid var(--vscode-panel-border); flex-wrap: wrap; }
  .intent-item:hover { background: var(--vscode-list-hoverBackground); }
  .status { flex-shrink: 0; }
  .title { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .id { font-size: 10px; opacity: 0.6; font-family: monospace; }
  .actions { display: flex; gap: 4px; }
  .actions button { background: none; border: none; cursor: pointer; font-size: 14px; padding: 2px; }
  .empty { text-align: center; padding: 20px; opacity: 0.6; }
  h3 { margin: 0 0 8px; font-size: 13px; }
</style></head><body>
  <h3>📋 Intents (${intents.length})</h3>
  ${intents.length === 0 ? '<div class="empty">No intents yet. Create one with <code>hief graph create</code></div>' : items}
  <script>
    const vscode = acquireVsCodeApi();
    function approve(id) { vscode.postMessage({ type: 'approve', id }); }
    function reject(id) { vscode.postMessage({ type: 'reject', id }); }
  </script>
</body></html>`;
  }

  private getKanbanHtml(intents: Intent[]): string {
    const columns = [
      { key: "draft", label: "📝 Draft" },
      { key: "approved", label: "✅ Approved" },
      { key: "in_progress", label: "🔨 In Progress" },
      { key: "in_review", label: "👀 In Review" },
      { key: "verified", label: "✔️ Verified" },
      { key: "merged", label: "🎉 Merged" },
    ];

    const colHtml = columns
      .map((col) => {
        const items = intents.filter((i) => i.status === col.key);
        const cards = items
          .map(
            (i) => `
          <div class="card" draggable="true" data-id="${i.id}">
            <div class="card-title">${escapeHtml(i.title)}</div>
            <div class="card-meta">
              <span class="card-id">${i.id}</span>
              <span class="card-kind">${i.kind}</span>
              <span class="card-priority priority-${i.priority}">${i.priority}</span>
            </div>
            ${
              col.key === "in_review"
                ? `<div class="card-actions">
                <button onclick="updateStatus('${i.id}', 'verified')" class="btn-sm approve">✅ Approve</button>
                <button onclick="updateStatus('${i.id}', 'rejected')" class="btn-sm reject">❌ Reject</button>
              </div>`
                : ""
            }
          </div>`
          )
          .join("");

        return `<div class="column">
          <div class="column-header">${col.label} <span class="count">${items.length}</span></div>
          <div class="column-body" data-status="${col.key}">${cards}</div>
        </div>`;
      })
      .join("");

    return `<!DOCTYPE html>
<html><head><style>
  body { font-family: var(--vscode-font-family); margin: 0; padding: 16px; display: flex; gap: 12px; overflow-x: auto; height: 100vh; box-sizing: border-box; }
  .column { min-width: 220px; max-width: 280px; flex: 1; display: flex; flex-direction: column; }
  .column-header { font-weight: 600; padding: 8px; border-bottom: 2px solid var(--vscode-panel-border); margin-bottom: 8px; }
  .count { background: var(--vscode-badge-background); color: var(--vscode-badge-foreground); padding: 1px 6px; border-radius: 10px; font-size: 11px; margin-left: 4px; }
  .column-body { flex: 1; overflow-y: auto; }
  .card { background: var(--vscode-editorWidget-background); border: 1px solid var(--vscode-panel-border); border-radius: 6px; padding: 10px; margin-bottom: 8px; cursor: grab; }
  .card:hover { border-color: var(--vscode-focusBorder); }
  .card-title { font-weight: 500; margin-bottom: 6px; }
  .card-meta { display: flex; gap: 6px; font-size: 11px; opacity: 0.7; flex-wrap: wrap; }
  .card-id { font-family: monospace; }
  .card-actions { margin-top: 8px; display: flex; gap: 4px; }
  .btn-sm { font-size: 11px; padding: 2px 8px; border: 1px solid var(--vscode-panel-border); border-radius: 4px; cursor: pointer; background: var(--vscode-button-secondaryBackground); color: var(--vscode-button-secondaryForeground); }
  .btn-sm.approve:hover { background: #22c55e33; }
  .btn-sm.reject:hover { background: #ef444433; }
  .priority-critical { color: #dc2626; font-weight: 600; }
  .priority-high { color: #ea580c; }
  .priority-medium { color: #ca8a04; }
  .priority-low { color: #16a34a; }
</style></head><body>
  ${colHtml}
  <script>
    const vscode = acquireVsCodeApi();
    function updateStatus(id, status) {
      vscode.postMessage({ type: 'updateStatus', id, status });
    }
  </script>
</body></html>`;
  }

  private getDetailedListHtml(intents: Intent[]): string {
    const rows = intents
      .map(
        (i) => `<tr>
        <td><code>${i.id}</code></td>
        <td>${i.kind}</td>
        <td>${escapeHtml(i.title)}</td>
        <td>${i.status}</td>
        <td class="priority-${i.priority}">${i.priority}</td>
        <td>${i.assigned_to || "—"}</td>
      </tr>`
      )
      .join("");

    return `<!DOCTYPE html>
<html><head><style>
  body { font-family: var(--vscode-font-family); padding: 16px; }
  table { width: 100%; border-collapse: collapse; }
  th, td { text-align: left; padding: 8px 12px; border-bottom: 1px solid var(--vscode-panel-border); }
  th { font-weight: 600; position: sticky; top: 0; background: var(--vscode-editor-background); }
  code { font-size: 11px; }
  .priority-critical { color: #dc2626; font-weight: 600; }
  .priority-high { color: #ea580c; }
</style></head><body>
  <h2>📋 All Intents (${intents.length})</h2>
  <table>
    <tr><th>ID</th><th>Kind</th><th>Title</th><th>Status</th><th>Priority</th><th>Assigned</th></tr>
    ${rows}
  </table>
</body></html>`;
  }
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
