# 🐻‍❄️〽️ minato: Fast, simple way to extract Bazel targets into a JSON Compilation Database
# Copyright (c) 2026 Noel <cutie@floofy.dev>, et al.
#
# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in all
# copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
# SOFTWARE.

"""Rules for wrapping the Minato binary as a runnable Bazel target.

Minato extracts Bazel targets into a JSON Compilation Database
(compile_commands.json) at the workspace root, enabling clangd and other
LSP-based tools to provide accurate code intelligence for C/C++/ObjC/CUDA
sources.

Load this file in your BUILD.bazel:
    load("@minato//:rcc.bzl", "minato")
"""

load("@rules_cc//cc:defs.bzl", "cc_binary")

def minato(name, targets = [], args = [], env = {}, visibility = []):
    """Defines a runnable target that invokes the Minato binary.

    Running `bazel run <name>` will execute minato over the specified targets
    and write `compile_commands.json` to the workspace root.

    Args:
        name: Name of the generated `sh_binary` target.
        targets: List of Bazel target patterns to extract compile commands from
            (e.g. `["//..."]`, `["//my/pkg:lib"]`). Defaults to all targets
            (`//...`) when left empty, which is minato's own default.

        args: Extra arguments forwarded to the minato binary after a `--`
            separator (e.g. `["--config=dbg"]`).

        env: Dictionary of environment variables made available to the binary
            at runtime.
    """

    all_args = []
    if len(args) > 0:
        env["MINATO_BAZEL_FLAGS"] = env.get("MINATO_BAZEL_FLAGS", "") + ";".join(args)

    for k, v in env.items():
        all_args.append("--env={}={}".format(k, v))

    all_args += targets

    cc_binary(
        name = name,
        srcs = select({
            "@platforms//os:linux": ["@minato//tools/wrapper:unix.cc"],
            "@platforms//os:macos": ["@minato//tools/wrapper:unix.cc"],
            "@platforms//os:windows": ["@minato//tools/wrapper:windows.cc"],
        }),
        data = ["@minato"],
        deps = ["@rules_cc//cc/runfiles"],
        args = all_args,
        copts = select({
            "@rules_cc//cc/compiler:msvc-cl": [
                "/Os", # optimize for speed
                "/EHs-c-", # disable C & C++ exceptions
                "/GR-", # disable RTTI
                "/std:c++latest", # use latest C++
                "/NODEFAULTLIB:msvcrt.lib",
                "/DWIN32_LEAN_AND_MEAN=1",
                "/D_CRT_SECURE_NO_WARNINGS"
            ],
            "@rules_cc//cc/compiler:clang-cl": [],
            "@rules_cc//cc/compiler:clang": [],
            "@rules_cc//cc/compiler:gcc": [],
            "//conditions:default": []
        }),
        visibility = visibility
    )
