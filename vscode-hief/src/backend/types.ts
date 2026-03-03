/**
 * TypeScript types mirroring HIEF's Rust structs.
 *
 * These are used for type-safe parsing of `hief --json` output.
 */

export interface Intent {
  id: string;
  kind: "feature" | "bug" | "refactor" | "spike" | "test" | "chore";
  title: string;
  description?: string;
  status:
  | "draft"
  | "approved"
  | "in_progress"
  | "in_review"
  | "verified"
  | "merged"
  | "rejected"
  | "blocked";
  priority: "critical" | "high" | "medium" | "low";
  criteria: string[];
  labels: string[];
  assigned_to?: string;
  created_at: number;
  updated_at: number;
}

export interface IntentWithDeps {
  intent: Intent;
  depends_on: Intent[];
  blocks: Intent[];
  all_deps_satisfied: boolean;
}

export interface IntentEdge {
  from_id: string;
  to_id: string;
  kind: "depends_on" | "blocks" | "implements" | "tests" | "related_to";
}

export interface DoctorReport {
  healthy: boolean;
  checks: DoctorCheck[];
  fixes_applied: number;
}

export interface DoctorCheck {
  name: string;
  status: "ok" | "warning" | "error";
  message: string;
  fixable: boolean;
  fixed: boolean;
}

export interface EvalResult {
  golden_set: string;
  overall_score: number;
  passed: boolean;
  cases: EvalCaseResult[];
}

export interface EvalCaseResult {
  case_name: string;
  priority: string;
  score: number;
  passed: boolean;
  violations: EvalViolation[];
}

export interface EvalViolation {
  kind: string;
  pattern: string;
  file: string;
}

export interface SearchResult {
  file_path: string;
  symbol_name?: string;
  symbol_kind?: string;
  parent_scope?: string;
  language: string;
  content: string;
  start_line: number;
  end_line: number;
  rank: number;
  snippet: string;
}

export interface IndexStats {
  total_files: number;
  total_chunks: number;
  db_size_bytes: number;
  last_indexed?: string;
  languages: Record<string, number>;
}

export interface GraphValidation {
  has_cycles: boolean;
  cycle_nodes: string[];
  auto_blocked: number;
}

/** Status display configuration */
export const STATUS_CONFIG: Record<
  Intent["status"],
  { icon: string; color: string; label: string }
> = {
  draft: { icon: "📝", color: "#6b7280", label: "Draft" },
  approved: { icon: "✅", color: "#22c55e", label: "Approved" },
  in_progress: { icon: "🔨", color: "#3b82f6", label: "In Progress" },
  in_review: { icon: "👀", color: "#a855f7", label: "In Review" },
  verified: { icon: "✔️", color: "#14b8a6", label: "Verified" },
  merged: { icon: "🎉", color: "#10b981", label: "Merged" },
  rejected: { icon: "❌", color: "#ef4444", label: "Rejected" },
  blocked: { icon: "🔒", color: "#f59e0b", label: "Blocked" },
};

export const PRIORITY_CONFIG: Record<
  Intent["priority"],
  { icon: string; color: string }
> = {
  critical: { icon: "🔴", color: "#dc2626" },
  high: { icon: "🟠", color: "#ea580c" },
  medium: { icon: "🟡", color: "#ca8a04" },
  low: { icon: "🟢", color: "#16a34a" },
};
