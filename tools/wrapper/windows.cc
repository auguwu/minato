// 🐻‍❄️〽️ minato: Fast, simple way to extract Bazel targets into a
// JSON Compilation Database Copyright (c) 2026 Noel <cutie@floofy.dev>, et al.
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

#include "cc/runfiles/runfiles.h"

#include <print>
#include <string>

#include <windows.h>

namespace {
auto widen(const char *s) -> std::wstring {
  int len = MultiByteToWideChar(CP_UTF8, 0, s, -1, nullptr, 0);
  std::wstring out(len - 1, L'\0');
  MultiByteToWideChar(CP_UTF8, 0, s, -1, out.data(), len);
  return out;
}

auto quote_arg(const wchar_t *arg) -> std::wstring {
  bool needs_quoting = false;
  for (const wchar_t *p = arg; *p; ++p) {
    if (*p == L' ' || *p == L'\t' || *p == L'"') {
      needs_quoting = true;
      break;
    }
  }

  if (!needs_quoting && *arg != L'\0') {
    return arg;
  }

  std::wstring result = L"\"";
  for (const wchar_t *p = arg;; ++p) {
    int num_backslashes = 0;
    while (*p == L'\\') {
      ++p;
      ++num_backslashes;
    }

    if (*p == L'\0') {
      result.append(num_backslashes * 2, L'\\');
      break;
    } else if (*p == L'"') {
      result.append(num_backslashes * 2 + 1, L'\\');
      result += L'"';
    } else {
      result.append(num_backslashes, L'\\');
      result += *p;
    }
  }

  result += L'"';
  return result;
}
} // namespace

auto main(int argc, char **argv) -> int {
  std::string error;
  auto runfiles = rules_cc::cc::runfiles::Runfiles::Create(argv[0], &error);
  if (!error.empty()) {
    std::println(stderr, "failed to create runfiles: {}", error);
    return 1;
  }

  auto minato = runfiles->Rlocation("minato+/minato.exe");
  if (minato.empty()) {
    std::println(stderr, "failed to find `minato` binary from runfiles");
    return 1;
  }

  // Parse args: extract --env=KEY=VALUE, forward the rest
  std::vector<std::pair<std::wstring, std::wstring>> env_overrides;
  std::vector<std::wstring> forward_args;

  for (int i = 1; i < argc; ++i) {
    std::string arg = argv[i];
    if (arg.starts_with("--env=")) {
      auto kv = arg.substr(6); // after "--env="
      auto eq = kv.find('=');
      if (eq == std::string::npos) {
        std::println(stderr, "malformed --env flag: {}", arg);
        return 1;
      }

      env_overrides.emplace_back(widen(kv.substr(0, eq).c_str()),
                                 widen(kv.substr(eq + 1).c_str()));
    } else {
      forward_args.push_back(widen(argv[i]));
    }
  }

  // Build command line
  std::wstring cmdline = L"minato";
  for (const auto &a : forward_args) {
    cmdline += L' ';
    cmdline += quote_arg(a.c_str());
  }

  // Build environment block
  wchar_t *existing_env = GetEnvironmentStringsW();
  std::vector<std::wstring> env_entries;

  for (const wchar_t *p = existing_env; *p; p += wcslen(p) + 1) {
    env_entries.emplace_back(p);
  }
  FreeEnvironmentStringsW(existing_env);

  // Add runfiles env vars
  for (const auto &[key, value] : runfiles->EnvVars()) {
    env_entries.push_back(widen(key.c_str()) + L"=" + widen(value.c_str()));
  }

  // Add --env= overrides
  for (const auto &[key, value] : env_overrides) {
    SetEnvironmentVariableW(key.c_str(), value.c_str());
    env_entries.push_back(key + L"=" + value);
  }

  // Double-null-terminated block
  std::wstring env_block;
  for (const auto &entry : env_entries) {
    env_block += entry;
    env_block += L'\0';
  }
  env_block += L'\0';

  std::vector<wchar_t> cmdbuf(cmdline.begin(), cmdline.end());
  cmdbuf.push_back(L'\0');

  auto wpath = widen(minato.c_str());

  STARTUPINFOW si = {};
  si.cb = sizeof(si);
  PROCESS_INFORMATION pi = {};

  if (!CreateProcessW(wpath.c_str(), cmdbuf.data(), nullptr, nullptr, FALSE,
                      CREATE_UNICODE_ENVIRONMENT, env_block.data(), nullptr,
                      &si, &pi)) {
    std::println(stderr, "failed to spawn minato: error {}", GetLastError());
    return 1;
  }

  WaitForSingleObject(pi.hProcess, INFINITE);

  DWORD exit_code = 1;
  GetExitCodeProcess(pi.hProcess, &exit_code);

  CloseHandle(pi.hProcess);
  CloseHandle(pi.hThread);

  return static_cast<int>(exit_code);
}
