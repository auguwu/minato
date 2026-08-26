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
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

#include "rules_cc/cc/runfiles/runfiles.h"

#include <violet/Filesystem.h>
#include <violet/Print.h>
#include <violet/Subprocess.h>

using namespace rules_cc::cc::runfiles;
using namespace violet;
namespace fs = violet::filesystem;

constexpr static Array<Str, 3> kPossibleLocations = {
    // clang-format off
    "_main/minato",
    "minato/minato",
    "minato+/minato",
    // clang-format on
};

auto main(Int32 argc, char** argv) -> Int32
{
    String error;
    auto* runfiles = Runfiles::Create(argv[0], &error);
    if (!error.empty()) {
        violet::PrintErrln("failed to create runfiles: {}", error);
        return 1;
    }

    String minato;
    for (auto possible: kPossibleLocations) {
        auto found = runfiles->Rlocation({possible.data()});
        if (!found.empty()) {
            auto result = fs::TryExists({found.data()});
            if (result.Err()) {
                violet::PrintErrln("warning: failed to probe file [{}]: {}", found, result.Error());
                continue;
            }

            if (*result) {
                minato = found;
                break;
            }
        }
    }

    if (minato.empty()) {
        violet::PrintErrln("failed to find `minato` binary from runfiles");
        return 1;
    }

    auto cmd = subprocess::Command(minato);
    for (const auto& [key, value]: runfiles->EnvVars()) {
        cmd = cmd.WithEnv(key, value);
    }

    for (Int32 i = 1; i < argc; ++i) {
        const Str arg(argv[i]);
        if (arg.starts_with("--env=")) {
            auto kv = arg.substr(6);
            auto eq = kv.find('=');
            if (eq == Str::npos) {
                violet::PrintErrln("malformed `--env` flag: {}", arg);
                return 1;
            }

            cmd = cmd.WithEnv(kv.substr(0, eq), kv.substr(eq + 1));
        } else {
            cmd = cmd.WithArg(arg);
        }
    }

    auto status = cmd.Status();
    if (status.Err()) {
        violet::PrintErrln("failed to exec minato: {}", status.Error());
        return 1;
    }

    return status->AsNative();
}
