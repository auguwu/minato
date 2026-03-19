// 🐻‍❄️〽️ minato: Fast, simple way to extract Bazel targets into a JSON Compilation Database
// Copyright (c) 2026 Noel <cutie@floofy.dev>, et al.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

#[macro_use]
extern crate tracing;

#[macro_use]
extern crate eyre;

pub mod bazel;
pub mod bep;
pub mod compdb;

use eyre::{Context, ContextCompat};
use facet::Facet;
use std::{
    collections::{HashMap, HashSet},
    env,
    io::ErrorKind,
    path::PathBuf,
    process::Stdio,
};
use tokio::{fs, process::Command};

#[derive(Facet)]
struct AspectEntry {
    file: String,
    arguments: Vec<String>,
}

const FLAGS_ENV: &str = "MINATO_BAZEL_FLAGS";

pub async fn extract(targets: &[String]) -> eyre::Result<compdb::Db> {
    let binary = bazel::find_binary()
        .context("failed to find `bazel` binary")?
        .context("no `bazel` binary found")?;

    let mut flags = Vec::new();
    if let Ok(mut data) =
        env::var(FLAGS_ENV).map(|s| s.split([';', ':']).map(ToOwned::to_owned).collect::<Vec<String>>()) &&
        !data.is_empty()
    {
        flags.append(&mut data);
    }

    let workspace_folder = bazel::run_command(&binary, ["info", "workspace"], /*inherit_stderr=*/ false)
        .await
        .map(PathBuf::from)?;

    let output_base = bazel::run_command(&binary, ["info", "output_base"], /*inherit_stderr=*/ false)
        .await
        .map(PathBuf::from)?;

    // Check if `external/` is symlinked (or junction'd) to the workspace folder
    let external = workspace_folder.join("external");
    match tokio::fs::symlink_metadata(&external).await {
        Ok(meta) if !meta.is_symlink() => {
            warn!(path = %external.display(), "deleting path as `external` will be symlinked");
            let _ = if meta.is_file() {
                tokio::fs::remove_file(&external).await
            } else {
                tokio::fs::remove_dir_all(&external).await
            };
        }

        Err(err) if err.kind() == ErrorKind::NotFound => {
            let output_external = output_base.join("external");

            #[cfg(unix)]
            if let Err(err) = tokio::fs::symlink(&output_external, &external).await {
                warn!(error = %err, "failed to symlink {} => {}", output_external.display(), external.display());
            } else {
                info!("symlinked {} => {}", output_external.display(), external.display());
            }

            #[cfg(windows)]
            if let Err(err) = tokio::fs::symlink_dir(&output_external, &external).await {
                warn!(error = %err, "failed to symlink {} => {}", output_external.display(), external.display());
            } else {
                info!("symlinked {} => {}", output_external.display(), external.display());
            }
        }

        Err(err) => {
            warn!(error = %err, "failed to collect symlink metadata for path [{}]", external.display());
        }

        _ => {}
    }

    let mut db = compdb::Db::new();
    info!("running bazel aspect...");

    let tempfile = tempfile::TempDir::new()?;
    let bep_path = tempfile.path().join("bep.json");

    let status = Command::new(&binary)
        .arg("build")
        .args(targets)
        .args([
            "--aspects=@minato//:aspect.bzl%minato_aspect",
            "--output_groups=db,required_inputs",
            "--noshow_progress",
            "--ui_event_filters=-info",
        ])
        .arg(format!("--build_event_json_file={}", bep_path.display()))
        .args(&flags)
        .stdout(Stdio::null())
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to spawn `bazel build`")?
        .wait()
        .await
        .context("failed to wait for child to finish")?;

    if !status.success() {
        bail!("`bazel build` with aspects failed, view logs above");
    }

    let content = fs::read_to_string(&bep_path)
        .await
        .context("failed to read BEP output")?;

    let output_files = collect_bep_output_files(&content);
    info!(files = output_files.len(), "found aspect output files");

    let ws = workspace_folder.display().to_string();
    for path in &output_files {
        let content = match tokio::fs::read_to_string(path).await {
            Ok(c) => c,
            Err(err) => {
                warn!(error = %err, path = %path.display(), "failed to read aspect output file");
                continue;
            }
        };

        let aspect_entries: Vec<AspectEntry> = match facet_json::from_str(&content) {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, "failed to parse aspect output");
                continue;
            }
        };

        for ae in aspect_entries {
            db.push(compdb::Entry {
                directory: ws.clone(),
                file: ae.file,
                arguments: ae.arguments,
            });
        }
    }

    db.sort_by_key(|a| a.file.to_ascii_lowercase());
    db.dedup_by(|a, b| a.file.eq_ignore_ascii_case(&b.file));

    info!(count = db.len(), "extraction completed");
    Ok(db)
}

fn collect_bep_output_files(bep_content: &str) -> Vec<PathBuf> {
    let mut named_sets: HashMap<String, bep::NamedSetOfFiles> = HashMap::new();
    let mut output_set_ids: Vec<String> = Vec::new();

    for line in bep_content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let event: bep::Event = match facet_json::from_str(trimmed) {
            Ok(e) => e,
            Err(_) => continue,
        };

        if let (Some(id), Some(ns)) = (event.id, event.named_set_of_files) {
            if let Some(ns_ref) = id.named_set {
                named_sets.insert(ns_ref.id, ns);
            }
        } else if let Some(completed) = event.completed {
            for group in completed.output_groups {
                if group.name == "db" {
                    for fs in group.file_sets {
                        output_set_ids.push(fs.id);
                    }
                }
            }
        }
    }

    let mut file_paths = Vec::new();
    let mut visited = HashSet::new();

    for id in &output_set_ids {
        collect_named_set_files(&named_sets, id, &mut file_paths, &mut visited);
    }

    file_paths
}

fn collect_named_set_files(
    named_sets: &HashMap<String, bep::NamedSetOfFiles>,
    set_id: &str,
    result: &mut Vec<PathBuf>,
    visited: &mut HashSet<String>,
) {
    if !visited.insert(set_id.to_owned()) {
        return;
    }

    let Some(set) = named_sets.get(set_id) else {
        return;
    };

    // Collect what we need before the recursive calls to satisfy the borrow checker.
    let uris: Vec<String> = set.files.iter().map(|f| f.uri.clone()).collect();
    let child_ids: Vec<String> = set.file_sets.iter().map(|r| r.id.clone()).collect();

    for uri in uris {
        // BEP URIs look like "file:///abs/path"; strip the scheme to get the path.
        let path = uri.strip_prefix("file://").unwrap_or(&uri).to_owned();
        result.push(PathBuf::from(path));
    }

    for child_id in child_ids {
        collect_named_set_files(named_sets, &child_id, result, visited);
    }
}
