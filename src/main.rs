//! HIEF — Hybrid Intent-Evaluation Framework
//!
//! A lightweight sidecar for AI coding agents providing:
//! - Persistent codebase index (AST-aware code search via FTS5)
//! - Intent tracking (dependency graph of tasks)
//! - Evaluation scoring (golden set quality checks)

mod cli;
mod config;
mod db;
mod docs;
mod errors;
mod eval;
mod graph;
mod index;
mod mcp;

use std::path::PathBuf;
use std::process;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use cli::{Cli, Commands, DocsCmd, EvalCmd, GoldenCmd, GraphCmd, HooksCmd, IndexCmd, McpCmd};
use config::Config;
use db::Database;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize tracing
    let filter = match cli.verbose {
        0 => "hief=info",
        1 => "hief=debug",
        _ => "hief=trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
        .with_target(false)
        .init();

    // Determine project root (current directory)
    let project_root = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("Error: cannot determine current directory: {}", e);
        process::exit(1);
    });

    // Run the command
    if let Err(e) = run(cli, project_root).await {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

async fn run(cli: Cli, project_root: PathBuf) -> anyhow::Result<()> {
    let json = cli.json;
    let config_path = project_root.join(&cli.config);

    match cli.command {
        Commands::Init => {
            cli::commands::init(&project_root).await?;
        }

        Commands::Version => {
            println!("hief {}", env!("CARGO_PKG_VERSION"));
        }

        Commands::Index(cmd) => {
            let config = Config::load(&config_path)?;
            let db_path = Config::db_path(&project_root);
            let db = Database::open(&db_path).await?;

            match cmd {
                IndexCmd::Build => {
                    cli::commands::index_build(&db, &project_root, &config, json).await?;
                }
                IndexCmd::Search {
                    query,
                    top_k,
                    language,
                    kind,
                } => {
                    cli::commands::index_search(
                        &db,
                        &query,
                        top_k,
                        language.as_deref(),
                        kind.as_deref(),
                        json,
                    )
                    .await?;
                }
                IndexCmd::Structural {
                    pattern,
                    language,
                    top_k,
                } => {
                    cli::commands::index_structural(
                        &project_root,
                        &pattern,
                        &language,
                        top_k,
                        json,
                    )?;
                }
                IndexCmd::Status => {
                    cli::commands::index_status(&db, &project_root, json).await?;
                }
            }
        }

        Commands::Graph(cmd) => {
            let _config = Config::load(&config_path)?;
            let db_path = Config::db_path(&project_root);
            let db = Database::open(&db_path).await?;

            match cmd {
                GraphCmd::Create {
                    kind,
                    title,
                    description,
                    priority,
                    depends_on,
                } => {
                    cli::commands::graph_create(
                        &db,
                        &kind,
                        &title,
                        description.as_deref(),
                        &priority,
                        depends_on.as_deref(),
                        json,
                    )
                    .await?;
                }
                GraphCmd::List { status, kind } => {
                    cli::commands::graph_list(&db, status.as_deref(), kind.as_deref(), json)
                        .await?;
                }
                GraphCmd::Show { id } => {
                    cli::commands::graph_show(&db, &id, json).await?;
                }
                GraphCmd::Update {
                    id,
                    status,
                    assign,
                } => {
                    cli::commands::graph_update(
                        &db,
                        &id,
                        status.as_deref(),
                        assign.as_deref(),
                        json,
                    )
                    .await?;
                }
                GraphCmd::Ready => {
                    cli::commands::graph_ready(&db, json).await?;
                }
                GraphCmd::Validate => {
                    cli::commands::graph_validate(&db, json).await?;
                }
            }
        }

        Commands::Eval(cmd) => {
            let config = Config::load(&config_path)?;
            let db_path = Config::db_path(&project_root);
            let db = Database::open(&db_path).await?;

            match cmd {
                EvalCmd::Run { golden, ci } => {
                    let exit_code = cli::commands::eval_run(
                        &db,
                        &project_root,
                        &config,
                        golden.as_deref(),
                        ci,
                        json,
                    )
                    .await?;
                    if exit_code != 0 {
                        process::exit(exit_code);
                    }
                }
                EvalCmd::Report { golden, limit } => {
                    cli::commands::eval_report(&db, &golden, limit, json).await?;
                }
                EvalCmd::Golden(GoldenCmd::List) => {
                    cli::commands::eval_golden_list(&project_root, &config, json)?;
                }
            }
        }

        Commands::Doctor { fix } => {
            let config = Config::load(&config_path)?;
            let db_path = Config::db_path(&project_root);
            let db = Database::open(&db_path).await?;

            cli::commands::doctor(&db, &project_root, &config_path, &config, fix, json).await?;
        }

        Commands::Upgrade => {
            cli::commands::upgrade(&project_root, &config_path, json).await?;
        }

        Commands::Hooks(cmd) => {
            match cmd {
                HooksCmd::Install => {
                    cli::commands::hooks_install(&project_root, json)?;
                }
                HooksCmd::Uninstall => {
                    cli::commands::hooks_uninstall(&project_root, json)?;
                }
                HooksCmd::Status => {
                    cli::commands::hooks_status(&project_root, json)?;
                }
            }
        }

        Commands::Docs(cmd) => {
            let config = Config::load(&config_path)?;

            match cmd {
                DocsCmd::Init => {
                    cli::commands::docs_init(&project_root, &config, json)?;
                }
                DocsCmd::Generate {
                    template,
                    name,
                    id,
                    output,
                    vars,
                    auto_populate,
                    force,
                } => {
                    cli::commands::docs_generate(
                        &project_root,
                        &config,
                        &template,
                        name.as_deref(),
                        id.as_deref(),
                        output.as_deref(),
                        &vars,
                        auto_populate,
                        force,
                        json,
                    )
                    .await?;
                }
                DocsCmd::List => {
                    cli::commands::docs_list(json)?;
                }
                DocsCmd::Check => {
                    cli::commands::docs_check(&project_root, &config, json)?;
                }
                DocsCmd::Variables { template } => {
                    cli::commands::docs_variables(&template, json)?;
                }
            }
        }

        Commands::Mcp(cmd) => {
            match cmd {
                McpCmd::Install { target, global } => {
                    cli::commands::mcp_install(
                        &project_root,
                        Some(target.as_str()),
                        global,
                        json,
                    )?;
                }
                McpCmd::Uninstall { target, global } => {
                    cli::commands::mcp_uninstall(
                        &project_root,
                        Some(target.as_str()),
                        global,
                        json,
                    )?;
                }
                McpCmd::Status => {
                    cli::commands::mcp_status(&project_root, json)?;
                }
            }
        }

        Commands::Serve(args) => {
            let config = Config::load(&config_path)?;
            let db_path = Config::db_path(&project_root);
            let db = Database::open(&db_path).await?;

            mcp::start(
                db,
                project_root,
                &config.serve,
                args.transport.as_deref(),
                args.port,
            )
            .await?;
        }
    }

    Ok(())
}
