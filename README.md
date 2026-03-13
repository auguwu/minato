### 🐻‍❄️〽️ Minato
#### *Fast, simple way to extract Bazel targets into a [JSON Compilation Database](https://clang.llvm.org/docs/JSONCompilationDatabase.html)*
Minato is a successor to [`hedronvision/bazel-compile-commands-extractor`](https://github.com/hedronvision/bazel-compile-commands-extractor), rewritten in Rust for significantly faster extraction on large codebases.

## How it works
Minato runs `bazel aquery` over the specified targets, filters for C/C++/ObjC/CUDA compile actions, strips output-only flags (`-o`, `-MF`, `-c`), and writes the result to `compile_commands.json` in your workspace root.

## Usage
Add the **minato** dependency in `MODULE.bazel`:

```python
bazel_dep(name = "minato", version = "<version>", dev_dependency = True)
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
