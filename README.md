### 🐻‍❄️〽️ Minato
#### *Fast, simple way to extract Bazel targets into a [JSON Compilation Database](https://clang.llvm.org/docs/JSONCompilationDatabase.html)*
Minato is a successor to [`hedronvision/bazel-compile-commands-extractor`](https://github.com/hedronvision/bazel-compile-commands-extractor), rewritten in Rust for significantly faster extraction on large codebases.

It also applies the following patches:

* [`hedronvision/bazel-compile-commands-extractor#279`](https://github.com/hedronvision/bazel-compile-commands-extractor/issues/279)
* [`hedronvision/bazel-compile-commands-extractor#273`](https://github.com/hedronvision/bazel-compile-commands-extractor/pull/273)
* [`hedronvision/bazel-compile-commands-extractor#229`](https://github.com/hedronvision/bazel-compile-commands-extractor/issues/229)

**Minato** doesn't support doing `targets = { "//target": ["--flag"] }` specified [here](https://github.com/hedronvision/bazel-compile-commands-extractor?tab=readme-ov-file#3-often-though-youll-want-to-specify-the-top-level-output-targets-you-care-about-andor-what-flags-they-individually-need-this-avoids-issues-where-some-targets-cant-be-built-on-their-own-they-need-configuration-on-the-command-line-or-by-a-parent-rule-an-example-of-the-latter-is-an-android_library-which-probably-cannot-be-built-independently-of-the-android_binary-that-configures-it) as myself uses this, but if people need it, a pull request can be submitted.

## How it works
Minato runs `bazel aquery` over the specified targets, filters for C/C++/ObjC/CUDA compile actions, strips output-only flags (`-o`, `-MF`, `-c`), and writes the result to `compile_commands.json` in your workspace root.

## Usage
Add the **minato** dependency in `MODULE.bazel`:

```python
# Minato depends on `rules_shell` if you're using the `minato` macro in `rcc.bzl`
bazel_dep(name = "rules_shell", version = "0.6.1", dev_dependency = True)
bazel_dep(name = "minato", dev_dependency = True)
git_override(
    module_name = "minato",
    commit = "<git commit>",
    remote = "https://github.com/auguwu/minato.git"
)
```

And in your BUILD.bazel, you can wrap the `minato` binary:

```python
load("@minato//:rcc.bzl", "minato")

minato(
    name = "rcc",

    # A list of targets to extract from. By default, it'll use `//...`.
    targets = [],

    # Additional arguments to passthrough, like `--config=dbg`.
    args = []
)
```

and run `bazel run rcc` and get a similar output.
