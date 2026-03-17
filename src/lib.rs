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
    collections::HashSet,
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Stdio,
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
    ".s", ".asm", ".S",

    // Headers
    ".h", ".hh", ".hpp", ".hxx", ".h++"
];

const HDR_SOURCE_EXTENSION_TO_LANGUAGE_ARGS: phf::Map<&'static str, &'static str> = phf::phf_map! {
    ".c" => "-xc",
    ".i" => "-xc",
    ".cc" => "-xc++",
    ".cpp" => "-xc++",
    ".cxx" => "-xc++",
    ".c++" => "-xc++",
    ".C" => "-xc++",
    ".CC" => "-xc++",
    ".cp" => "-xc++",
    ".CPP" => "-xc++",
    ".C++" => "-xc++",
    ".CXX" => "-xc++",
    ".ii" => "-xc++",
    ".m" => "-xobjective-c",
    ".mm" => "-xobjective-c++",
    ".M" => "-xobjective-c++",
    ".cu" => "-xcuda",
    ".cui" => "-xcuda",
    ".s" => "-xassembler",
    ".asm" => "-xassembler",
    ".S" => "-xassembler-with-cpp",
    ".h" => "-xc-header",
    ".hh" => "-xc++-header",
    ".hpp" => "-xc++-header",
    ".hxx" => "-xc++-header",
    ".h++" => "-xc++-header"
};

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
    /// Create a synthetic action for a header file by cloning this action's arguments
    /// but swapping the source file for `header`.
    fn synthesize_for_header(&self, header: &str) -> AQueryAction {
        let mut args = self.arguments.clone();
        for arg in args.iter_mut().skip(1) {
            if !arg.starts_with('-') && SOURCE_EXTENSIONS.iter().any(|ext| arg.ends_with(ext)) {
                *arg = header.to_owned();
                break;
            }
        }

        AQueryAction {
            mnemonic: self.mnemonic.clone(),
            arguments: args,
        }
    }

    pub fn convert(&self, workspace: &Path) -> Option<CompilationDatabaseEntry> {
        let file = self.find_source_file()?;
        let mut args = self.strip_compile_args();

        // https://github.com/clangd/clangd/issues/1173
        // https://github.com/clangd/clangd/issues/1263
        if args.iter().any(|arg| {
            !((arg.starts_with("-x") || arg.starts_with("--language")) ||
                ["-objc", "-objc++", "/tc", "/tp"].contains(&arg.to_ascii_lowercase().as_str()))
        }) {
            if let Some(compiler) = self.arguments.first() &&
                compiler.ends_with("cl.exe")
            {
                args.insert(1, "/TP".to_owned());
            } else {
                let path = PathBuf::from(&file);
                let extension = path.extension().and_then(OsStr::to_str).unwrap_or_default();

                if let Some(lang) = HDR_SOURCE_EXTENSION_TO_LANGUAGE_ARGS.get(&format!(".{extension}")) {
                    args.insert(1, lang.to_string());
                }
            }
        }

        Some(CompilationDatabaseEntry {
            directory: workspace.display().to_string(),
            file,
            arguments: args,
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

pub async fn extract(targets: Vec<String>) -> eyre::Result<Vec<CompilationDatabaseEntry>> {
    let Some(bazel) = find_bazel_binary()? else {
        return Err(eyre!("failed to find `bazel` binary"));
    };

    let mut extra_flags = Vec::new();
    if let Ok(mut data) =
        env::var("MINATO_BAZEL_FLAGS").map(|s| s.split(';').map(ToOwned::to_owned).collect::<Vec<String>>()) &&
        !data.is_empty()
    {
        extra_flags.append(&mut data);
    }

    let workspace = get_workspace_root(bazel.as_path()).await?;
    info!(workspace = %workspace.display(), "using workspace folder");

    let mut entries = Vec::new();
    let mut fallback_args: Option<Vec<String>> = None;
    for target in targets {
        let mut actions = extract_target(
            &bazel,
            &target,
            extra_flags.as_slice(),
            workspace.as_path(),
            fallback_args.as_deref(),
        )
        .await?;
        if fallback_args.is_none() {
            fallback_args = actions.first().map(|e| e.arguments.clone());
        }
        entries.append(&mut actions);
    }

    entries.dedup_by(|a, b| a.file.eq_ignore_ascii_case(&b.file));

    info!(count = entries.len(), "extraction completed");
    Ok(entries)
}

/// Convert a Bazel source-file label (`//pkg/sub:file.h`) to its absolute path.
/// Labels from external repositories (`@repo//...`) are ignored.
fn bazel_label_to_path(label: &str, workspace: &Path) -> Option<PathBuf> {
    let label = label.trim();
    if !label.starts_with("//") {
        return None; // skip external repos (@...) and malformed labels
    }

    let label = &label[2..]; // strip leading "//"
    let (pkg, file) = match label.find(':') {
        Some(pos) => (&label[..pos], &label[pos + 1..]),
        None => {
            // No colon — treat the last path component as the file name.
            let slash_pos = label.rfind('/')?;
            (&label[..slash_pos], &label[slash_pos + 1..])
        }
    };

    let mut path = workspace.to_path_buf();
    for component in pkg.split('/') {
        if !component.is_empty() {
            path.push(component);
        }
    }

    path.push(file);
    Some(path)
}

/// Query Bazel for all files listed in `hdrs` attributes of transitive deps of `target`.
/// Returns absolute paths to header files that live inside the workspace.
async fn query_header_files(
    bazel: &Path,
    target: &str,
    extra_flags: &[String],
    workspace: &Path,
) -> eyre::Result<Vec<PathBuf>> {
    let expr = format!("labels(hdrs, deps({target}))");
    debug!(%expr, %target, "running `bazel query` for header files...");

    let output = Command::new(bazel)
        .arg("query")
        .arg(&expr)
        .arg("--output=label")
        .arg("--ui_event_filters=-info")
        .arg("--noshow_progress")
        .args(extra_flags)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .await
        .context("failed to run `bazel query` for header files")?;

    if !output.status.success() {
        warn!("bazel query for header files failed; skipping header-only entries");
        return Ok(Vec::new());
    }

    let text = String::from_utf8(output.stdout).context("`bazel query` output was not valid UTF-8")?;

    Ok(text
        .lines()
        .filter_map(|line| bazel_label_to_path(line, workspace))
        .filter(|p| {
            p.extension()
                .and_then(OsStr::to_str)
                .map(|ext| {
                    [".h", ".hh", ".hpp", ".hxx", ".h++"]
                        .iter()
                        .any(|hext| *hext == format!(".{ext}"))
                })
                .unwrap_or(false)
        })
        .collect())
}

async fn extract_target(
    bazel: &Path,
    target: &str,
    extra_flags: &[String],
    workspace: &Path,
    fallback_args: Option<&[String]>,
) -> eyre::Result<Vec<CompilationDatabaseEntry>> {
    let expr = format!("mnemonic('(Objc|Cpp|Cuda)Compile', deps({target}))");
    debug!(%expr, %target, "running `bazel aquery`...");

    let output = Command::new(bazel)
        .arg("aquery")
        .arg(&expr)
        .arg("--output=jsonproto")
        .arg("--ui_event_filters=-info")
        .arg("--noshow_progress")
        .arg("--include_artifacts=false") // We only need arguments, not artifact paths.
        .arg("--features=-compiler_param_file") // Expand param files so all flags are visible in `arguments`.
        .arg("--features=-layering_check") // Suppress layering-check actions that produce no source file.
        .arg("--host_features=-compiler_param_file")
        .arg("--host_features=-layering_check")
        .args(extra_flags)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to run `bazel aquery`")?
        .wait_with_output()
        .await
        .context("failed to run `bazel aquery`")?;

    if !output.status.success() {
        bail!("`bazel aquery` failed for target {}, view logs above", target)
    }

    let blob = String::from_utf8(output.stdout).context("`bazel aquery` output was not valid UTF-8")?;
    let output: AQueryOutput =
        facet_json::from_str(&blob).map_err(|e| eyre!("failed to parse aquery output: {e}"))?;

    info!(%target, actions = output.actions.len(), "processing compilation actions...");
    let mut entries = Vec::new();

    for action in &output.actions {
        match action.convert(workspace) {
            Some(entry) => entries.push(entry),
            None => {
                warn!("skipping sourceless compilation action");
            }
        }
    }

    // Header-only targets produce no compile actions, so we query their `hdrs`
    // separately and synthesize entries using the first real action as a flag
    // template. If this target has no actions, we fall back to args inherited
    // from a previously processed target, or a bare `-x` entry as a last resort.
    let hdr_paths = query_header_files(bazel, target, extra_flags, workspace).await?;
    if !hdr_paths.is_empty() {
        let existing: HashSet<String> = entries.iter().map(|e| e.file.clone()).collect();
        let template = output.actions.first();
        let mut added = 0usize;

        for hdr_path in &hdr_paths {
            let file = hdr_path.display().to_string();
            if existing.contains(&file) {
                continue;
            }

            let entry = if let Some(tmpl) = template {
                tmpl.synthesize_for_header(&file).convert(workspace)
            } else if let Some(fb) = fallback_args {
                // No actions for this target — inherit flags from a previously seen action.
                AQueryAction {
                    mnemonic: String::new(),
                    arguments: fb.to_vec(),
                }
                .synthesize_for_header(&file)
                .convert(workspace)
            } else {
                // No compile actions anywhere — synthesize a bare minimal entry.
                let ext = hdr_path.extension().and_then(OsStr::to_str).unwrap_or_default();
                let lang_flag = HDR_SOURCE_EXTENSION_TO_LANGUAGE_ARGS
                    .get(&format!(".{ext}"))
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "-xc++-header".to_owned());

                Some(CompilationDatabaseEntry {
                    directory: workspace.display().to_string(),
                    file: file.clone(),
                    arguments: vec!["clang".to_owned(), lang_flag, file],
                })
            };

            if let Some(e) = entry {
                entries.push(e);
                added += 1;
            }
        }

        if added > 0 {
            info!(%target, added, "added header-only entries");
        }
    }

    Ok(entries)
}
