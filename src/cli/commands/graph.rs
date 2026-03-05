//! `hief graph` — intent graph management commands.

use crate::db::Database;
use crate::errors::Result;
use crate::graph;
use crate::graph::edges::IntentEdge;
use crate::graph::intent::Intent;

/// Create a new intent.
pub async fn graph_create(
    db: &Database,
    kind: &str,
    title: &str,
    description: Option<&str>,
    priority: &str,
    depends_on: Option<&str>,
    json: bool,
) -> Result<()> {
    let intent = Intent::new(
        kind,
        title,
        description.map(String::from),
        Some(priority.to_string()),
    );

    graph::create_intent(db, &intent).await?;

    // Add dependency edges if specified
    if let Some(deps) = depends_on {
        for dep_id in deps.split(',').map(|s| s.trim()) {
            if !dep_id.is_empty() {
                let edge = IntentEdge::depends_on(&intent.id, dep_id);
                graph::add_edge(db, &edge).await?;
            }
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&intent).unwrap());
    } else {
        println!("✅ Created intent: {} ({})", intent.id, intent.title);
    }

    Ok(())
}

/// List intents.
pub async fn graph_list(
    db: &Database,
    status: Option<&str>,
    kind: Option<&str>,
    json: bool,
) -> Result<()> {
    let intents = graph::list_intents(db, status, kind).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&intents).unwrap());
    } else if intents.is_empty() {
        println!("No intents found.");
    } else {
        println!("📋 Intents ({}):\n", intents.len());
        for i in &intents {
            let assigned = i.assigned_to.as_deref().unwrap_or("unassigned");
            println!(
                "  {} [{}] {} — {} ({})",
                status_icon(&i.status),
                i.kind,
                i.title,
                i.status,
                assigned,
            );
            println!("    ID: {}", i.id);
        }
    }

    Ok(())
}

/// Show intent details (supports short ID prefix resolution).
pub async fn graph_show(db: &Database, id: &str, json: bool) -> Result<()> {
    let resolved_id = graph::resolve_id(db, id).await?;
    let intent_with_deps = graph::get_intent_with_deps(db, &resolved_id).await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&intent_with_deps).unwrap()
        );
    } else {
        let i = &intent_with_deps.intent;
        println!("📌 Intent: {}", i.title);
        println!("   ID: {}", i.id);
        println!("   Kind: {}", i.kind);
        println!("   Status: {} {}", status_icon(&i.status), i.status);
        println!("   Priority: {}", i.priority);
        if let Some(desc) = &i.description {
            println!("   Description: {}", desc);
        }
        if let Some(assigned) = &i.assigned_to {
            println!("   Assigned to: {}", assigned);
        }
        if !i.criteria.is_empty() {
            println!("   Criteria:");
            for c in &i.criteria {
                println!("     - {}", c);
            }
        }
        println!(
            "   Dependencies satisfied: {}",
            if intent_with_deps.all_deps_satisfied {
                "✅ yes"
            } else {
                "❌ no"
            }
        );
        if !intent_with_deps.depends_on.is_empty() {
            println!("   Depends on:");
            for dep in &intent_with_deps.depends_on {
                println!(
                    "     {} {} — {} ({})",
                    status_icon(&dep.status),
                    dep.title,
                    dep.status,
                    dep.id,
                );
            }
        }
        if !intent_with_deps.blocks.is_empty() {
            println!("   Blocks:");
            for blk in &intent_with_deps.blocks {
                println!("     {} ({})", blk.title, blk.id);
            }
        }
    }

    Ok(())
}

/// Update an intent (supports short ID prefix resolution).
pub async fn graph_update(
    db: &Database,
    id: &str,
    status: Option<&str>,
    assign: Option<&str>,
    json: bool,
) -> Result<()> {
    let resolved_id = graph::resolve_id(db, id).await?;
    let id = resolved_id.as_str();

    if let Some(new_status) = status {
        graph::update_status(db, id, new_status).await?;
        if !json {
            println!("✅ Updated status to '{}'", new_status);
        }
    }

    if let Some(assignee) = assign {
        graph::assign_intent(db, id, assignee).await?;
        if !json {
            println!("✅ Assigned to '{}'", assignee);
        }
    }

    if json {
        let intent = graph::get_intent(db, id).await?;
        println!("{}", serde_json::to_string_pretty(&intent).unwrap());
    }

    Ok(())
}

/// Show ready intents.
pub async fn graph_ready(db: &Database, json: bool) -> Result<()> {
    let intents = graph::ready_intents(db).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&intents).unwrap());
    } else if intents.is_empty() {
        println!("No intents ready for work.");
    } else {
        println!("🚀 Ready intents ({}):\n", intents.len());
        for i in &intents {
            println!("  [{}] {} — {}", i.kind, i.title, i.priority);
            println!("    ID: {}", i.id);
        }
    }

    Ok(())
}

/// Validate graph integrity.
pub async fn graph_validate(db: &Database, json: bool) -> Result<()> {
    let validation = graph::validate_graph(db).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&validation).unwrap());
    } else {
        if validation.has_cycles {
            println!(
                "⚠️  Cycles detected in {} nodes:",
                validation.cycle_nodes.len()
            );
            for node in &validation.cycle_nodes {
                println!("    - {}", node);
            }
        } else {
            println!("✅ No cycles detected");
        }
        if validation.auto_blocked > 0 {
            println!(
                "🔒 {} intents auto-blocked (depend on rejected intents)",
                validation.auto_blocked
            );
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) fn status_icon(status: &str) -> &'static str {
    match status {
        "draft" => "📝",
        "approved" => "✅",
        "in_progress" => "🔨",
        "in_review" => "👀",
        "verified" => "✔️",
        "merged" => "🎉",
        "rejected" => "❌",
        "blocked" => "🔒",
        _ => "❓",
    }
}
