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

use eyre::Context;
use facet::Facet;
use is_executable::IsExecutable;
use std::{
    env,
    path::{Path, PathBuf},
    str::FromStr,
};
use tokio::process::Command;
use tracing::{info, warn};

/// A list of source file extensions that identify a real compilation input (not a header
/// being pre-compiled)
#[rustfmt::skip]
const SOURCE_EXTENSIONS: &[&str] = &[
    // C
    ".c", ".i",

    // C++
    ".cc", ".cpp", ".cxx", ".c++", ".C", ".CC", ".cp", ".CPP", ".C++", ".CXX", ".ii",

    // Objective-C
    ".m",

    // Objective-C++
    ".mm", ".M",

    // CUDA
    ".cu", ".cui",

    // Assembly (with C pre-processor)
    ".s", ".asm", ".S"
];

#[derive(Facet)]
struct AQueryOutput {
    actions: Vec<AQueryAction>,
}

#[derive(Facet)]
struct AQueryAction {
    mnemonic: String,
    arguments: Vec<String>,
}

impl AQueryAction {
    pub fn convert(&self, workspace: &Path) -> Option<CompilationDatabaseEntry> {
        let file = self.find_source_file()?;
        Some(CompilationDatabaseEntry {
            directory: workspace.display().to_string(),
            file,
            arguments: self.strip_compile_args(),
        })
    }

    fn find_source_file(&self) -> Option<String> {
        self.arguments
            .iter()
            .skip(1)
            .find(|arg| !arg.starts_with('-') && SOURCE_EXTENSIONS.iter().any(|ext| arg.ends_with(ext)))
            .cloned()
    }

    fn strip_compile_args(&self) -> Vec<String> {
        let mut result = Vec::with_capacity(self.arguments.len());
        let mut skip_next = false;

        for arg in self.arguments.iter() {
            if skip_next {
                skip_next = false;
                continue;
            }

            match arg.as_str() {
                "-c" => continue,
                "-o" | "-MF" => {
                    skip_next = true;
                    continue;
                }

                _ => result.push(arg.to_owned()),
            }
        }

        result
    }
}

/// A entry in a compilation database.
#[derive(Debug, Facet)]
pub struct CompilationDatabaseEntry {
    /// The working directory of the compilation. All relative paths in
    /// [`arguments`][Self::arguments] are relative to this.
    pub directory: String,

    /// The file itself.
    pub file: String,

    /// The arguments that is supplied to compile this single file.
    pub arguments: Vec<String>,
}

pub fn find_bazel_binary() -> eyre::Result<Option<PathBuf>> {
    if let Ok(bazel) = env::var("BAZEL").map(|x| unsafe {
        // Safety: `PathBuf` will never return an error since the `FromStr`
        // implementation's `Error` GAT is `Infalliable`.
        PathBuf::from_str(&x).unwrap_unchecked()
    }) {
        debug!("bazel binary: lookup with `$BAZEL` environment variable");
        if bazel.is_file() {
            if !bazel.is_executable() {
                bail!("`bazel` binary in [{}] is not executable", bazel.display());
            }

            return Ok(Some(bazel));
        }

        bail!("`bazel` binary in [{}] was not a real file", bazel.display());
    }

    for candidate in ["bazelisk", "bazel"] {
        debug!(candidate, "bazel binary: lookup from `$PATH`");
        if let Ok(bazel) = which::which(candidate) {
            if bazel.is_file() {
                if !bazel.is_executable() {
                    bail!("binary lookup `{}`: not executable", bazel.display());
                }

                return Ok(Some(bazel));
            }

            bail!("binary lookup `{}`: not a real file", bazel.display());
        }
    }

    Ok(None)
}

async fn get_workspace_root(bazel: &Path) -> eyre::Result<PathBuf> {
    let output = Command::new(bazel)
        .args(["info", "workspace"])
        .output()
        .await
        .context("failed to run `bazel info workspace`")?;

    if !output.status.success() {
        debug!(
            "`bazel info workspace` output :: stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );

        debug!(
            "`bazel info workspace` output :: stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        bail!("`bazel info workspace` failed");
    }

    Ok(PathBuf::from(
        String::from_utf8(output.stdout)
            .context("`bazel info workspace` produced non UTF-8 output")
            .map(|s| s.trim().to_owned())?,
    ))
}

pub async fn extract(
    targets: Vec<String>,
    extra_flags: Vec<String>,
) -> eyre::Result<Vec<CompilationDatabaseEntry>> {
    let Some(bazel) = find_bazel_binary()? else {
        return Err(eyre!("failed to find `bazel` binary"));
    };

    let workspace = get_workspace_root(bazel.as_path()).await?;
    info!(workspace = %workspace.display(), "using workspace folder");

    let mut entries = Vec::new();
    for target in targets {
        let mut actions = extract_target(&bazel, &target, extra_flags.as_slice(), workspace.as_path()).await?;
        entries.append(&mut actions);
    }

    info!(count = entries.len(), "extraction completed");
    Ok(entries)
}

async fn extract_target(
    bazel: &Path,
    target: &str,
    extra_flags: &[String],
    workspace: &Path,
) -> eyre::Result<Vec<CompilationDatabaseEntry>> {
    let expr = format!("mnemonic('(Objc|Cpp|Cuda)Compile', deps({target}))");
    debug!(%expr, %target, "running `bazel aquery`...");

    let output = Command::new(bazel)
        .arg("aquery")
        .arg(&expr)
        .arg("--output=jsonproto")
        .arg("--include_artifacts=false") // We only need arguments, not artifact paths.
        .arg("--features=-compiler_param_file") // Expand param files so all flags are visible in `arguments`.
        .arg("--features=-layering_check") // Suppress layering-check actions that produce no source file.
        .args(extra_flags)
        .output()
        .await
        .context("failed to run `bazel aquery`")?;

    if !output.status.success() {
        debug!(
            "`bazel aquery` output :: stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );

        debug!(
            "`bazel aquery` output :: stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        bail!("`bazel aquery` failed for target {}", target)
    }

    let blob = String::from_utf8(output.stdout).context("`bazel aquery` output was not valid UTF-8")?;

    let output: AQueryOutput =
        facet_json::from_str(&blob).map_err(|e| eyre!("failed to parse aquery output: {e}"))?;

    info!(%target, actions = output.actions.len(), "processing compilation actions...");
    let mut entries = Vec::new();

    for action in output.actions {
        match action.convert(workspace) {
            Some(entry) => entries.push(entry),
            None => {
                warn!("skipping sourceless compilation action");
            }
        }
    }

    Ok(entries)
}
