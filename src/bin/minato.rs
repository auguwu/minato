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

use eyre::Context;
use facet::Facet;
use figue as args;
use std::{io::Write, path::PathBuf};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Facet)]
struct Args {
    /// A list of Bazel targets to generate a JSON Compilation Database from
    #[facet(args::positional, default = vec!["//...".to_owned()])]
    targets: Vec<String>,

    /// If provided, the directory to change into. Useful for testing.
    #[facet(args::named, args::short = 'd', default)]
    chdir: Option<PathBuf>,

    /// If set, this will pretty print the JSON compilation database
    /// instead of writing on-disk.
    #[facet(args::named, args::short = 'p', default = false)]
    print: bool,

    #[facet(flatten)]
    builtins: args::FigueBuiltins,
}

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_writer(std::io::stderr)
        .init();

    color_eyre::install()?;

    let config = figue::builder::<Args>()
        .unwrap()
        .cli(|cli| cli.args(std::env::args().skip(1)))
        .help(|help| {
            help.program_name(env!("CARGO_PKG_NAME"))
                .version(env!("CARGO_PKG_VERSION"))
                .description("A fast, simple way to extract Bazel targets into a JSON Compilation Database")
        })
        .build();

    let args: Args = figue::Driver::new(config).run().unwrap();
    tracing::debug!(?args, "arguments provided");

    // Before doing anything, if we have `BUILD_WORKSPACE_DIRECTORY` defined, then we
    // are being executed via `bazel run`.
    //
    // Also, if `--chdir` isn't specified, then do this so that we can save
    // a syscall.
    if let Ok(env) = std::env::var("BUILD_WORKSPACE_DIRECTORY") &&
        args.chdir.is_none()
    {
        tracing::trace!(
            env = "BUILD_WORKSPACE_DIRECTORY",
            "found `$BUILD_WORKSPACE_DIRECTORY` [{}] -- chrooting...",
            env
        );

        std::env::set_current_dir(env).context("failed to change working directory")?;
    } else if let Some(ref dir) = args.chdir {
        std::env::set_current_dir(dir).context("failed to change working directory")?;
    }

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?
        .block_on(real_main(args))
}

async fn real_main(Args { targets, print, .. }: Args) -> eyre::Result<()> {
    let compdb = minato::extract(targets.as_slice()).await?;
    if print {
        let mut stdout = std::io::stdout().lock();
        facet_json::to_writer_std_pretty(&mut stdout, &compdb).unwrap();
        writeln!(stdout).unwrap();

        return Ok(());
    }

    let mut file = std::fs::File::options()
        .create(true)
        .write(true)
        .truncate(true)
        .open("compile_commands.json")
        .context("failed to open or create `compile_commands.json`")?;

    facet_json::to_writer_std_pretty(&mut file, &compdb)
        .context("failed to write JSON compilation database to `compile_commands.json`")
}
