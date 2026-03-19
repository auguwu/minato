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
//
//! A module that does Bazel-related actions.

use eyre::Context;
use is_executable::IsExecutable;
use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Stdio,
    str::FromStr,
};
use tokio::process::Command;

pub fn find_binary() -> eyre::Result<Option<PathBuf>> {
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

pub async fn run_command(
    bazel: &Path,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    inherit_stderr: bool,
) -> eyre::Result<String> {
    let output = Command::new(bazel)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(if inherit_stderr {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .stdin(Stdio::null())
        .spawn()
        .context("failed to run bazel command")?
        .wait_with_output()
        .await
        .context("failed to run bazel command")?;

    if !output.status.success() {
        bail!("command failed; view logs above")
    }

    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_owned())
        .context("produced non UTF-8 output")
}
