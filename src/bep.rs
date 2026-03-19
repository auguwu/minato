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

use facet::Facet;

#[derive(Facet)]
pub struct Event {
    #[facet(default)]
    pub id: Option<EventId>,

    #[facet(rename = "namedSetOfFiles", default)]
    pub named_set_of_files: Option<NamedSetOfFiles>,

    #[facet(default)]
    pub completed: Option<TargetComplete>,
}

#[derive(Facet)]
pub struct EventId {
    #[facet(rename = "namedSet", default)]
    pub named_set: Option<SetRef>,
}

#[derive(Facet)]
pub struct SetRef {
    pub id: String,
}

#[derive(Facet)]
pub struct NamedSetOfFiles {
    #[facet(default)]
    pub files: Vec<FileEntry>,

    #[facet(rename = "fileSets", default)]
    pub file_sets: Vec<SetRef>,
}

#[derive(Facet)]
pub struct FileEntry {
    pub uri: String,
}

#[derive(Facet)]
pub struct TargetComplete {
    #[facet(rename = "outputGroup", default)]
    pub output_groups: Vec<OutputGroup>,
}

#[derive(Facet)]
pub struct OutputGroup {
    pub name: String,

    #[facet(rename = "fileSets", default)]
    pub file_sets: Vec<SetRef>,
}
