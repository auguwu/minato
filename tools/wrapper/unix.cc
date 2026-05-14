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

#include <array>
#include <cstdlib>
#include <cstring>
#include <format>
#include <iostream>
#include <string>
#include <utility>
#include <vector>

#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

#if defined(__has_include) && __has_include(<TargetConditionals.h>)
#include <TargetConditionals.h>

#if TARGET_OS_MAC && TARGET_OS_OSX
#ifndef APPLE_MACOS
#define APPLE_MACOS 1
#endif
#endif
#else
#define APPLE_MACOS 0
#endif

constexpr static std::array<const char*, 3> kPossibleLocations = { "_main/minato", "minato/minato", "minato+/minato" };

namespace {

auto tryExists(const std::string& path) -> std::pair<bool, int>
{
#if APPLE_MACOS
#define O_PATH O_RDONLY
#endif

    auto fd = ::open(path.c_str(), O_PATH | O_NOFOLLOW);
    if (fd == -1) {
        if (errno == ENOENT) {
            return std::make_pair(false, 0);
        }

        return std::make_pair(false, errno);
    }

    struct stat st{ };
    if (::fstat(fd, &st) < 0) {
        return std::make_pair(false, errno);
    }

    return { true, 0 };
}

} // namespace

#define eprintln(fmt, ...) ::std::cerr << ::std::format(fmt __VA_OPT__(, ) __VA_ARGS__) << '\n'
#define println(fmt, ...) ::std::cout << ::std::format(fmt __VA_OPT__(, ) __VA_ARGS__) << '\n'

auto main(int argc, char** argv) -> int
{
    std::string error;
    auto* runfiles = rules_cc::cc::runfiles::Runfiles::Create(argv[0], &error);
    if (!error.empty()) {
        eprintln("failed to create runfiles: {}", error);
        return 1;
    }

    std::string minato;
    for (const auto* possible: kPossibleLocations) {
        auto found = runfiles->Rlocation(possible);
        if (!found.empty()) {
            auto [exists, error] = tryExists(found);
            if (!exists && error != 0) {
                eprintln("warning: failed to probe file [{}]: {}", found, ::strerror(error));
                continue;
            }

            if (exists) {
                minato = found;
                break;
            }
        }
    }

    if (minato.empty()) {
        eprintln("failed to find `minato` binary from runfiles");
        return 1;
    }

    // Set runfiles environment variables
    for (const auto& [key, value]: runfiles->EnvVars()) {
        ::setenv(key.c_str(), value.c_str(), 1);
    }

    std::vector<char*> fwdArgs;

    // NOLINTNEXTLINE(cppcoreguidelines-pro-type-const-cast)
    fwdArgs.push_back(const_cast<char*>(minato.c_str()));

    for (int i = 1; i < argc; ++i) {
        std::string arg(argv[i]);
        if (arg.starts_with("--env=")) {
            auto kv = arg.substr(6); // "--env="
            auto eq = kv.find('=');
            if (eq == std::string::npos) {
                eprintln("malformed `--env` flag: {}", arg);
                return 1;
            }

            auto key = kv.substr(0, eq);
            auto value = kv.substr(eq + 1);
            ::setenv(key.c_str(), value.c_str(), 1);
        } else {
            fwdArgs.push_back(argv[i]);
        }
    }

    fwdArgs.push_back(nullptr);
    ::execv(minato.c_str(), fwdArgs.data());

    eprintln("failed to exec minato: {}", ::strerror(errno));
    return 1;
}
