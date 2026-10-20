use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use open_pdm_core::{HandlerRegistry, Index, Project};

#[derive(Parser)]
#[command(name = "openpdm", version, about = "OpenPDM CLI (first pass)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Summarise a project's repository.
    Info { repo: PathBuf },

    /// List revisions, newest first.
    Revisions {
        repo: PathBuf,
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,
    },

    /// List files at a revision (defaults to HEAD).
    Files {
        repo: PathBuf,
        #[arg(long)]
        rev: Option<String>,
    },

    /// Show metadata for one file via the handler registry.
    Meta {
        repo: PathBuf,
        path: String,
        #[arg(long)]
        rev: Option<String>,
    },

    /// Tessellate a file and report mesh statistics.
    Mesh {
        repo: PathBuf,
        path: String,
        #[arg(long)]
        rev: Option<String>,
    },

    /// List the registered CAD handlers.
    Handlers,

    /// Build (or refresh) the SQLite index for a project.
    Index {
        repo: PathBuf,
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(short = 'n', long, default_value_t = 200)]
        limit: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Info { repo } => info(&repo),
        Command::Revisions { repo, limit } => revisions(&repo, limit),
        Command::Files { repo, rev } => files(&repo, rev.as_deref()),
        Command::Meta { repo, path, rev } => meta(&repo, &path, rev.as_deref()),
        Command::Mesh { repo, path, rev } => mesh(&repo, &path, rev.as_deref()),
        Command::Handlers => handlers(),
        Command::Index { repo, db, limit } => index(&repo, db, limit),
    }
}

fn open(repo: &PathBuf) -> Result<Project> {
    Project::open(repo).with_context(|| format!("opening project at {}", repo.display()))
}

fn info(repo: &PathBuf) -> Result<()> {
    let project = open(repo)?;
    println!("root:      {}", project.root().display());
    println!("bare:      {}", project.is_bare());
    println!(
        "branch:    {}",
        project.branch().unwrap_or_else(|| "(detached)".into())
    );
    match project.head_revision() {
        Ok(rev) => {
            println!("head:      {} {}", rev.short, rev.summary);
            println!("author:    {} <{}>", rev.author, rev.email);
            println!("files@HEAD: {}", project.files_at(None)?.len());
        }
        Err(_) => println!("head:      (no commits yet)"),
    }
    Ok(())
}

fn revisions(repo: &PathBuf, limit: usize) -> Result<()> {
    let project = open(repo)?;
    let revs = project.revisions(limit)?;
    if revs.is_empty() {
        println!("(no revisions)");
        return Ok(());
    }
    println!("{:<10} {:<12} {:<20} summary", "SHORT", "WHEN", "AUTHOR");
    for r in revs {
        println!(
            "{:<10} {:<12} {:<20} {}",
            r.short,
            fmt_time(r.time),
            truncate(&r.author, 20),
            r.summary
        );
    }
    Ok(())
}

fn files(repo: &PathBuf, rev: Option<&str>) -> Result<()> {
    let project = open(repo)?;
    let files = project.files_at(rev)?;
    println!("{} file(s) at {}", files.len(), rev.unwrap_or("HEAD"));
    for f in files {
        println!(
            "  {:<10} {:>12}  {}{}",
            &f.oid[..8.min(f.oid.len())],
            f.size,
            f.path,
            if f.is_binary { "  [binary]" } else { "" }
        );
    }
    Ok(())
}

fn meta(repo: &PathBuf, path: &str, rev: Option<&str>) -> Result<()> {
    let project = open(repo)?;
    let bytes = project.blob(rev, path)?;
    let registry = HandlerRegistry::builtin();
    let handler = registry.resolve(path);
    let meta = handler.metadata(path, &bytes)?;
    println!("handler: {}", handler.name());
    println!("format:  {}", meta.format);
    for (k, v) in &meta.attributes {
        println!("  {k:<14} {v}");
    }
    Ok(())
}

fn mesh(repo: &PathBuf, path: &str, rev: Option<&str>) -> Result<()> {
    let project = open(repo)?;
    let bytes = project.blob(rev, path)?;
    let registry = HandlerRegistry::builtin();
    let handler = registry.resolve(path);
    match handler.mesh(path, &bytes)? {
        Some(mesh) => {
            println!("handler:   {}", handler.name());
            println!("vertices:  {}", mesh.vertex_count());
            println!("triangles: {}", mesh.triangle_count());
            if let Some((min, max)) = mesh.bounds() {
                println!(
                    "bounds:    [{:.3}, {:.3}, {:.3}] .. [{:.3}, {:.3}, {:.3}]",
                    min[0], min[1], min[2], max[0], max[1], max[2]
                );
            }
            println!("volume:    {:.4}", mesh.volume());
        }
        None => println!("handler '{}' does not produce a mesh", handler.name()),
    }
    Ok(())
}

fn handlers() -> Result<()> {
    let registry = HandlerRegistry::builtin();
    println!("{:<10} extensions", "HANDLER");
    for (name, exts) in registry.list() {
        let exts = if exts.is_empty() {
            "(fallback)".to_string()
        } else {
            exts.join(", ")
        };
        println!("{name:<10} {exts}");
    }
    Ok(())
}

fn index(repo: &PathBuf, db: Option<PathBuf>, limit: usize) -> Result<()> {
    let project = open(repo)?;
    let db = db.unwrap_or_else(|| project.root().join(".openpdm").join("index.pdmdb"));
    let mut index = Index::open(&db)?;
    let stats = index.index(&project, limit)?;
    println!("index:          {}", db.display());
    println!("schema version: {}", index.schema_version()?);
    println!("repositories:   {}", stats.repositories);
    println!("revisions:      {}", stats.revisions);
    println!("file-revisions: {}", stats.file_revisions);
    println!("unique blobs:   {}", stats.unique_blobs);
    println!("unique bytes:   {}", human(stats.unique_bytes));
    println!("referenced:     {}", human(stats.referenced_bytes));
    println!("dedup saved:    {}", human(stats.dedup_saved_bytes));
    Ok(())
}

fn fmt_time(secs: i64) -> String {
    // Minimal, dependency-free relative/absolute formatting.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let delta = now - secs;
    if delta < 60 {
        "just now".to_string()
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86_400 {
        format!("{}h ago", delta / 3600)
    } else {
        format!("{}d ago", delta / 86_400)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
