use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use nte_core::config::LauncherConfig;
use nte_core::game;
use nte_core::game::check::CheckOptions;
use nte_core::game::download::ApplyOptions;
use nte_core::manifest::Manifest;
use nte_core::{update, version};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().cloned() else {
        return usage();
    };
    if command == "keyring-check" {
        nte_core::attendance::check_keyring()?;
        println!("Keyring 存取与清理检查通过");
        return Ok(());
    }
    if command == "attendance" {
        let dir = flag_value(&args, "--config")
            .map(PathBuf::from)
            .unwrap_or_else(nte_core::attendance::default_dir);
        let status = nte_core::attendance::Service::new(dir).run(None)?;
        if status.accounts.is_empty() {
            anyhow::bail!("请先登录塔吉多账号");
        }
        let mut failed = false;
        for account in status.accounts {
            if let Some(error) = &account.attention {
                eprintln!("{}: {}", account.masked_phone, error);
                failed = true;
            }
            if account.signed_today {
                println!("{}: 今日已签到", account.masked_phone);
            } else {
                failed = true;
                println!("{}: 今日未签到", account.masked_phone);
            }
            if let Some(report) = &account.last_report {
                for step in &report.steps {
                    println!("{}: {}: {}", account.masked_phone, step.label, step.message);
                }
            }
        }
        if failed {
            anyhow::bail!("签到未全部成功");
        }
        return Ok(());
    }
    let root = flag_value(&args, "--root")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("missing --root <launcher install dir>"))?;
    let config = LauncherConfig::load(&root)
        .with_context(|| format!("load {}/Config/Config.ini", root.display()))?;

    match command.as_str() {
        "check" => check(&config, args.contains(&"--quick".to_string())),
        "apply" => apply(&config, flag_value(&args, "--only")),
        "doctor" => doctor(&config),
        "game-check" => game_check(&config, &args),
        "game-apply" => game_apply(&config, &args),
        _ => usage(),
    }
}

fn check(config: &LauncherConfig, quick: bool) -> Result<()> {
    println!("[local]  version={} build={}", config.version, config.build);
    let remote = version::fetch(&config.version_url).context("fetch Version.ini")?;
    println!("[remote] version={} build={}", remote.version, remote.build);
    let url = remote
        .file_list_url
        .clone()
        .ok_or_else(|| anyhow!("remote Version.ini has no FileListURL"))?;
    let manifest = Manifest::from_url(&url).context("fetch AllFiles.xml")?;
    println!(
        "[manifest] product={:?} files={} base={:?}",
        manifest.product_version,
        manifest.files.len(),
        manifest.base_url
    );
    let report = update::check(&config.root, &manifest, quick)?;
    println!(
        "[check] ok={} missing={} changed={} bytes={}",
        report.ok,
        report.missing.len(),
        report.changed.len(),
        report.total_missing_bytes()
    );
    for f in report.missing.iter().take(20) {
        println!("  missing {}", f.path);
    }
    for f in report.changed.iter().take(20) {
        println!("  changed {}", f.path);
    }
    Ok(())
}

fn apply(config: &LauncherConfig, only: Option<String>) -> Result<()> {
    let remote = version::fetch(&config.version_url).context("fetch Version.ini")?;
    let url = remote
        .file_list_url
        .clone()
        .ok_or_else(|| anyhow!("remote Version.ini has no FileListURL"))?;
    let base_dir = url
        .rsplit_once('/')
        .map(|(d, _)| d.to_string())
        .unwrap_or(url.clone());
    let manifest = Manifest::from_url(&url).context("fetch AllFiles.xml")?;
    let report = update::check(&config.root, &manifest, true)?;
    let mut todo: Vec<_> = report.missing.into_iter().chain(report.changed).collect();
    if let Some(filter) = only {
        todo.retain(|f| f.path.contains(&filter));
    }
    println!("[apply] files={}", todo.len());
    for f in &todo {
        match update::apply_file(&config.root, &base_dir, f) {
            Ok(n) => println!("  ok {} <- {} B", f.path, n),
            Err(e) => println!("  FAIL {}: {e}", f.path),
        }
    }
    Ok(())
}

fn game_config(config: &LauncherConfig) -> Result<game::GameUpdateConfig> {
    game::GameUpdateConfig::load(&config.root, config.game_id).with_context(|| {
        format!(
            "load PatcherConfig under {}",
            config.root.join("ResFilesM").display()
        )
    })
}

fn game_check(launcher: &LauncherConfig, args: &[String]) -> Result<()> {
    let cfg = game_config(launcher)?;
    let opts = CheckOptions {
        full: args.iter().any(|a| a == "--full"),
        include_optional: args.iter().any(|a| a == "--include-optional"),
    };
    let plan = game::check::check(&cfg, opts).context("check game resources")?;
    println!(
        "[game] local={} remote={} hash={} branch={} mode={}",
        if plan.local_version.is_empty() {
            "?"
        } else {
            &plan.local_version
        },
        plan.remote_version,
        plan.remote_hash,
        cfg.branch,
        if opts.full { "full" } else { "quick" }
    );
    println!(
        "[plan] checked={} update={} unverified={} download_bytes={}",
        plan.checked,
        plan.items.len(),
        plan.unverified.len(),
        plan.total_bytes()
    );
    for item in &plan.items {
        let patch = if item.patches.is_empty() {
            String::new()
        } else {
            let steps = item
                .patches
                .iter()
                .map(|patch| format!("{}.{}", patch.md5, patch.size))
                .collect::<Vec<_>>()
                .join("+");
            format!(" patch={steps}")
        };
        println!(
            "  {:<9} {:>12}  {}{}",
            item.reason.as_str(),
            item.size,
            item.path,
            patch
        );
    }
    for item in plan.unverified.iter().take(40) {
        println!(
            "  {:<9} {:>12}  {}  (use --full to hash)",
            item.reason.as_str(),
            item.size,
            item.path
        );
    }
    if plan.unverified.len() > 40 {
        println!("  ... {} more unverified", plan.unverified.len() - 40);
    }
    if plan.items.is_empty() {
        if plan.unverified.is_empty() {
            println!("[plan] game resources are up to date");
        } else {
            println!(
                "[plan] no updates; {} file(s) unverified (use --full to hash)",
                plan.unverified.len()
            );
        }
    }
    Ok(())
}

fn game_apply(launcher: &LauncherConfig, args: &[String]) -> Result<()> {
    let cfg = game_config(launcher)?;
    let check_opts = CheckOptions {
        full: args.iter().any(|a| a == "--full"),
        include_optional: args.iter().any(|a| a == "--include-optional"),
    };
    let plan = game::check::check(&cfg, check_opts).context("check game resources")?;
    if plan.items.is_empty() {
        println!(
            "[apply] nothing to do (local={} remote={})",
            plan.local_version, plan.remote_version
        );
        return Ok(());
    }
    let opts = ApplyOptions {
        dry_run: args.iter().any(|a| a == "--dry-run"),
        only: flag_value(args, "--only"),
        force: args.iter().any(|a| a == "--force"),
        update_ledger: true,
    };
    let selected: Vec<_> = plan
        .items
        .iter()
        .filter(|i| match &opts.only {
            Some(f) => i.path.contains(f),
            None => true,
        })
        .collect();
    let selected_bytes: u64 = selected.iter().map(|i| i.download_size).sum();
    if selected.is_empty() {
        println!("[apply] no files match the selected filter");
        return Ok(());
    }
    if opts.dry_run {
        println!(
            "[apply] dry-run: {} files, {} bytes",
            selected.len(),
            selected_bytes
        );
        for item in &selected {
            println!("  {} ({} B)", item.path, item.download_size);
        }
        return Ok(());
    }
    println!(
        "[apply] files={} bytes={} branch={}",
        selected.len(),
        selected_bytes,
        cfg.branch
    );
    let report = game::download::apply_plan(&cfg, &plan, &opts);
    for r in &report.results {
        if r.ok {
            if r.skipped {
                println!("  skip {}", r.path);
            } else {
                println!("  ok   {} ({} B downloaded)", r.path, r.bytes);
            }
        } else {
            println!(
                "  FAIL {}: {}",
                r.path,
                r.error.as_deref().unwrap_or("unknown")
            );
        }
    }
    println!(
        "[apply] ok={} failed={} skipped={} downloaded={} B",
        report.results.iter().filter(|r| r.ok).count(),
        report.failed,
        report.skipped,
        report.downloaded_bytes
    );
    if let Some(e) = &report.ledger_error {
        println!("  WARN ledger update failed: {e}");
    }
    if report.failed > 0 {
        return Err(anyhow!("{} game update file(s) failed", report.failed));
    }
    Ok(())
}

fn doctor(config: &LauncherConfig) -> Result<()> {
    println!("root       = {}", config.root.display());
    println!("launcher   = {}", config.launcher);
    println!("patcher    = {}", config.patcher);
    println!("client     = {}", config.client);
    println!("game id    = {}", config.game_id);
    println!("version    = {} ({})", config.version, config.build);
    println!("version url= {}", config.version_url);
    println!("branch     = {}", config.branch);
    println!("game dir   = {}", config.game_dir().display());
    println!("ignored    = {:?}", config.patcher_ignored_sections);
    println!("mutex      = {}", config.mutex_name);
    Ok(())
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn usage() -> Result<()> {
    println!("nte-cli attendance [--config <签到配置目录>]\nnte-cli keyring-check");
    println!("nte-cli <check|apply|doctor|game-check|game-apply> --root <launcher install dir> [--quick] [--full] [--include-optional] [--only <substr>] [--dry-run] [--force]");
    Ok(())
}

#[cfg(test)]
mod tests;
