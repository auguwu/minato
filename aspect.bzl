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

"""
The main aspect for Minato.
"""

load(
    "@rules_cc//cc:action_names.bzl",
    "ASSEMBLE_ACTION_NAME",
    "CPP_COMPILE_ACTION_NAME",
    "C_COMPILE_ACTION_NAME",
    "OBJCPP_COMPILE_ACTION_NAME",
    "OBJC_COMPILE_ACTION_NAME",
    "PREPROCESS_ASSEMBLE_ACTION_NAME",
)
load("@rules_cc//cc:find_cc_toolchain.bzl", "find_cc_toolchain", "use_cc_toolchain")
load("@rules_cc//cc/common:cc_common.bzl", "cc_common")
load("@rules_cc//cc/common:cc_info.bzl", "CcInfo")

_EXTENSION_TO_ACTION = {
    "c": C_COMPILE_ACTION_NAME,
    "i": C_COMPILE_ACTION_NAME,
    "cc": CPP_COMPILE_ACTION_NAME,
    "cpp": CPP_COMPILE_ACTION_NAME,
    "cxx": CPP_COMPILE_ACTION_NAME,
    "c++": CPP_COMPILE_ACTION_NAME,
    "ii": CPP_COMPILE_ACTION_NAME,
    "m": OBJC_COMPILE_ACTION_NAME,
    "mm": OBJCPP_COMPILE_ACTION_NAME,
    "cu": CPP_COMPILE_ACTION_NAME,
    "s": ASSEMBLE_ACTION_NAME,
    "asm": ASSEMBLE_ACTION_NAME,
    "S": PREPROCESS_ASSEMBLE_ACTION_NAME,
}

_EXT_TO_X_FLAG = {
    "c": "-xc",
    "i": "-xc",
    "cc": "-xc++",
    "cpp": "-xc++",
    "cxx": "-xc++",
    "c++": "-xc++",
    "ii": "-xc++",
    "m": "-xobjective-c",
    "mm": "-xobjective-c++",
    "cu": "-xcuda",
    "s": "-xassembler",
    "asm": "-xassembler",
    "S": "-xassembler-with-cpp",
    "h": "-xc-header",
    "hh": "-xc++-header",
    "hpp": "-xc++-header",
    "hxx": "-xc++-header",
    "h++": "-xc++-header",
}

# MSVC only distinguishes C vs C++. Assembly is handled by ml/ml64, not cl.
# Headers have no direct compilation mode; empty string means "omit the flag".
_EXT_TO_X_FLAG_MSVC = {
    "c": "/Tc",
    "i": "/Tc",
    "cc": "/Tp",
    "cpp": "/Tp",
    "cxx": "/Tp",
    "c++": "/Tp",
    "ii": "/Tp",
    "s": "",
    "asm": "",
    "S": "",
    "h": "",
    "hh": "",
    "hpp": "",
    "hxx": "",
    "h++": "",
}

_SRC_EXTS = ["c", "i", "cc", "cpp", "cxx", "c++", "ii", "m", "mm", "cu", "s", "asm", "S"]
_HDR_EXTS = ["h", "hh", "hpp", "hxx", "h++"]
_CXX_EXTS = ["cc", "cpp", "cxx", "c++", "ii", "mm", "cu"]

CompileCommandInfo = provider(
    "Transitive compile command fragment files produced by the minato aspect.",
    fields = {
        "files": "depset of JSON fragment files",
        "required_inputs": "depset of generated files (e.g. genrule outputs used as headers) that must be built before the compdb is usable",
    },
)

def _expand_copts(ctx, flags, attr_name):
    return [ctx.expand_make_variables(attr_name, flag, {}) for flag in flags]

def _is_msvc(toolchain):
    """Returns True when the toolchain drives an MSVC-style compiler (cl.exe or clang-cl)."""
    return toolchain.compiler == "msvc-cl" or toolchain.compiler == "clang-cl"

def _make_compilation_entry(
        toolchain,
        feature_configuration,
        compilation_context,
        file,
        action,
        x,
        extra_flags,
        msvc):
    variables = cc_common.create_compile_variables(
        feature_configuration = feature_configuration,
        cc_toolchain = toolchain,
        source_file = file.path,
        include_directories = compilation_context.includes,
        quote_include_directories = compilation_context.quote_includes,
        system_include_directories = compilation_context.system_includes,
        framework_include_directories = compilation_context.framework_includes,
        preprocessor_defines = depset(
            transitive = [compilation_context.defines, compilation_context.local_defines],
        ),
        user_compile_flags = extra_flags,
    )

    command = cc_common.get_memory_inefficient_command_line(
        feature_configuration = feature_configuration,
        action_name = action,
        variables = variables,
    )

    arguments = list(command)

    if x == None:
        # Auto-detect C vs C++ for plain `.h` headers based on the toolchain
        # or a standard-version flag already present in the command line.
        if msvc:
            has_cxx_std_flag = any([
                arg.startswith("/std:c++")
                for arg in arguments
            ])

            is_cxx_compiler = "++" in toolchain.compiler_executable
            x = "/Tp" if (has_cxx_std_flag or is_cxx_compiler) else "/Tc"
        else:
            has_cxx_std_flag = any([
                arg.startswith("-std=c++") or arg.startswith("--std=c++") or
                arg.startswith("-std=gnu++") or arg.startswith("--std=gnu++")
                for arg in arguments
            ])

            is_cxx_compiler = "++" in toolchain.compiler_executable
            x = "-xc++-header" if (has_cxx_std_flag or is_cxx_compiler) else "-xc-header"

    if msvc:
        if x:
            # /Tp and /Tc must appear before the filename. The filename is
            # already the last element in `arguments` from
            # get_memory_inefficient_command_line, so insert the flag just
            # before it.
            return {
                "file": file.path,
                "arguments": [toolchain.compiler_executable] + arguments[:-1] + [x, arguments[-1]],
            }
        else:
            return {
                "file": file.path,
                "arguments": [toolchain.compiler_executable] + arguments,
            }
    else:
        return {
            "file": file.path,
            "arguments": [toolchain.compiler_executable, x] + arguments,
        }

def _aspect_impl(target, ctx):
    transitive_files = []
    transitive_required = []
    for attr in ["deps", "implementation_deps"]:
        for dep in getattr(ctx.rule.attr, attr, []):
            if CompileCommandInfo in dep:
                transitive_files.append(dep[CompileCommandInfo].files)
                transitive_required.append(dep[CompileCommandInfo].required_inputs)

    if CcInfo not in target:
        info = CompileCommandInfo(
            files = depset(transitive = transitive_files),
            required_inputs = depset(transitive = transitive_required),
        )
        return [
            info,
            OutputGroupInfo(
                db = info.files,
                required_inputs = info.required_inputs,
            ),
        ]

    toolchain = find_cc_toolchain(ctx)
    msvc = _is_msvc(toolchain)

    unsupported_features = list(ctx.disabled_features)
    for feat in ["layering_check", "compiler_param_file"]:
        if feat not in unsupported_features:
            unsupported_features.append(feat)

    feature_configuration = cc_common.configure_features(
        ctx = ctx,
        cc_toolchain = toolchain,
        requested_features = ctx.features,
        unsupported_features = unsupported_features,
    )

    compilation_context = target[CcInfo].compilation_context

    global_copts = list(ctx.fragments.cpp.copts)
    global_cxxopts = list(ctx.fragments.cpp.cxxopts)
    global_conlyopts = list(ctx.fragments.cpp.conlyopts)

    rule_copts = []
    if hasattr(ctx.rule.attr, "copts"):
        rule_copts = _expand_copts(ctx, ctx.rule.attr.copts, "copts")

    rule_cxxopts = []
    if hasattr(ctx.rule.attr, "cxxopts"):
        rule_cxxopts = _expand_copts(ctx, ctx.rule.attr.cxxopts, "cxxopts")

    rule_conlyopts = []
    if hasattr(ctx.rule.attr, "conlyopts"):
        rule_conlyopts = _expand_copts(ctx, ctx.rule.attr.conlyopts, "conlyopts")

    c_extra_flags = global_copts + global_conlyopts + rule_copts + rule_conlyopts
    cxx_extra_flags = global_copts + global_cxxopts + rule_copts + rule_cxxopts

    x_flag_map = _EXT_TO_X_FLAG_MSVC if msvc else _EXT_TO_X_FLAG

    srcs = []
    if hasattr(ctx.rule.attr, "srcs"):
        for label in ctx.rule.attr.srcs:
            for file in label.files.to_list():
                if file.extension in _SRC_EXTS:
                    srcs.append(file)

    hdrs = []
    required_inputs = []
    if hasattr(ctx.rule.attr, "hdrs"):
        for label in ctx.rule.attr.hdrs:
            for file in label.files.to_list():
                if file.extension in _HDR_EXTS:
                    hdrs.append(file)
                    if not file.is_source:
                        required_inputs.append(file)

    entries = []
    for file in srcs:
        ext = file.extension
        entries.append(_make_compilation_entry(
            toolchain,
            feature_configuration,
            compilation_context,
            file,
            _EXTENSION_TO_ACTION.get(ext, CPP_COMPILE_ACTION_NAME),
            x_flag_map.get(ext, "/Tp" if msvc else "-xc++"),
            cxx_extra_flags if ext in _CXX_EXTS else c_extra_flags,
            msvc,
        ))

    for file in hdrs:
        entries.append(_make_compilation_entry(
            toolchain,
            feature_configuration,
            compilation_context,
            file,
            CPP_COMPILE_ACTION_NAME,
            None,
            cxx_extra_flags,
            msvc,
        ))

    fragment = ctx.actions.declare_file("%s_db.json" % ctx.label.name)
    ctx.actions.write(fragment, json.encode(entries))

    req_depset = depset(direct = required_inputs, transitive = transitive_required)
    info = CompileCommandInfo(
        files = depset(direct = [fragment], transitive = transitive_files),
        required_inputs = req_depset,
    )
    return [
        info,
        OutputGroupInfo(
            db = info.files,
            required_inputs = info.required_inputs,
        ),
    ]

minato_aspect = aspect(
    implementation = _aspect_impl,
    fragments = ["cpp"],
    attr_aspects = ["deps", "implementation_deps"],
    toolchains = use_cc_toolchain(),
    attrs = {
        "_cc_toolchain": attr.label(
            default = "@bazel_tools//tools/cpp:current_cc_toolchain",
        ),
    },
)
