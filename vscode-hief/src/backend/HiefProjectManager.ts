/**
 * Backend wrapper around the `hief` CLI.
 *
 * All commands are executed with `--json` for structured output.
 * This is the single communication layer between the VS Code extension
 * and the HIEF Rust binary.
 */

import { exec } from "child_process";
import { promisify } from "util";
import type { Intent, DoctorReport, EvalResult, IntentWithDeps, IndexStats, SearchResult } from "./types";

const execAsync = promisify(exec);

export class HiefProjectManager {
  constructor(private workspaceRoot: string) { }

  /** Execute a hief CLI command and return parsed JSON. */
  private async run<T>(args: string): Promise<T> {
    const { stdout } = await execAsync(`hief --json ${args}`, {
      cwd: this.workspaceRoot,
      timeout: 30_000,
    });
    return JSON.parse(stdout) as T;
  }

  // -----------------------------------------------------------------------
  // Intent operations
  // -----------------------------------------------------------------------

  async listIntents(status?: string, kind?: string): Promise<Intent[]> {
    let args = "graph list";
    if (status) args += ` --status ${status}`;
    if (kind) args += ` --kind ${kind}`;
    return this.run<Intent[]>(args);
  }

  async showIntent(id: string): Promise<IntentWithDeps> {
    return this.run<IntentWithDeps>(`graph show ${id}`);
  }

  async createIntent(
    kind: string,
    title: string,
    priority: string = "medium",
    description?: string,
    dependsOn?: string
  ): Promise<Intent> {
    let args = `graph create --kind ${kind} --title "${title}" --priority ${priority}`;
    if (description) args += ` --description "${description}"`;
    if (dependsOn) args += ` --depends-on "${dependsOn}"`;
    return this.run<Intent>(args);
  }

  async updateIntent(
    id: string,
    status?: string,
    assignee?: string
  ): Promise<Intent> {
    let args = `graph update ${id}`;
    if (status) args += ` --status ${status}`;
    if (assignee) args += ` --assign ${assignee}`;
    return this.run<Intent>(args);
  }

  async readyIntents(): Promise<Intent[]> {
    return this.run<Intent[]>("graph ready");
  }

  // -----------------------------------------------------------------------
  // Index operations
  // -----------------------------------------------------------------------

  async searchCode(query: string, topK: number = 10): Promise<any[]> {
    return this.run<any[]>(`index search "${query}" -k ${topK}`);
  }

  async indexStatus(): Promise<IndexStats> {
    return this.run<IndexStats>("index status");
  }

  async indexBuild(): Promise<any> {
    return this.run<any>("index build");
  }

  async structuralSearch(
    pattern: string,
    language: string,
    topK: number = 50
  ): Promise<SearchResult[]> {
    return this.run<SearchResult[]>(
      `index structural "${pattern}" --language ${language} -k ${topK}`
    );
  }

  // -----------------------------------------------------------------------
  // Eval operations
  // -----------------------------------------------------------------------

  async runEval(golden?: string): Promise<EvalResult[]> {
    let args = "eval run";
    if (golden) args += ` --golden ${golden}`;
    return this.run<EvalResult[]>(args);
  }

  // -----------------------------------------------------------------------
  // Doctor
  // -----------------------------------------------------------------------

  async runDoctor(fix: boolean = false): Promise<DoctorReport> {
    let args = "doctor";
    if (fix) args += " --fix";
    return this.run<DoctorReport>(args);
  }

  // -----------------------------------------------------------------------
  // Hooks
  // -----------------------------------------------------------------------

  async hooksStatus(): Promise<any[]> {
    return this.run<any[]>("hooks status");
  }

  async hooksInstall(): Promise<any> {
    return this.run<any>("hooks install");
  }
}
