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
{
  mkShell,
  lib,
  stdenv,
  stdenvAdapters,
  ## rust
  rust-bin,
  cargo-nextest,
  cargo-machete,
  cargo-expand,
  cargo-deny,
  ## c++
  llvmPackages_22,
  ## bazel
  bazel_8,
  bazel-buildtools,
  ## os-specific
  ### linux
  mold,
}: let
  llvm = llvmPackages_22;
  clang-tools = llvm.clang-tools.override {
    enableLibcxx = true;
  };

  llvmStdenv =
    (
      if stdenv.hostPlatform.isLinux
      then stdenvAdapters.useMoldLinker
      else lib.id
    )
    llvm.libcxxStdenv;

  packages =
    [
      llvm.compiler-rt
      llvm.libcxx

      llvm.bintools
      llvm.lldb
      clang-tools

      cargo-nextest
      cargo-machete
      cargo-expand
      cargo-deny

      (rust-bin.fromRustupToolchainFile ../rust-toolchain.toml)

      bazel-buildtools
      bazel_8
    ]
    ++ (lib.optionals stdenv.isLinux [mold]);

  mkShell' = mkShell.override {
    stdenv = llvmStdenv;
  };
in
  mkShell' {
    inherit packages;

    name = "minato-dev";
    shellHook = ''
      export BAZEL_COPTS="-I${llvm.compiler-rt.dev}/include"
      export BAZEL_CXXOPTS="-xc++:-nostdinc++:-isystem:${llvm.libcxx.dev}/include/c++/v1"
      export BAZEL_LINKOPTS="-L${llvm.libcxx}/lib:-lc++:-lc++abi"
    '';
  }
