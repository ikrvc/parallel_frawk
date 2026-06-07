# Parallel AWK

`parallel_frawk` extends the original frawk project with automatic AWK program parallelization. The implementation introduces a static parallelizability analyzer that determines whether an AWK script can be safely executed in parallel while preserving the behavior of sequential execution.

The work was developed as part of the Master's thesis *Design and Implementation of Parallelized AWK*. The analyzer detects global state dependencies, identifies reduction operations for aggregatable variables, and automatically falls back to sequential execution when safe parallelization cannot be guaranteed.

## Main Features

- Static analysis of AWK programs before execution.
- Detection of global variables and cross-record dependencies.
- Automatic identification of reduction operations:
  - Addition (`+`)
  - Multiplication (`*`)
  - Logical AND (`&&`)
  - Logical OR (`||`)
  - Last-value assignment
- Parallel execution using thread-local variable copies.
- Automatic fallback to sequential execution for non-parallelizable programs.
- Support for selected associative array parallelization patterns.
- Analysis of common AWK built-in functions affecting parallelization.

## Installation

### Prerequisites

- Rust (stable toolchain)
- Cargo

### Build from Source
Cranelift-based JIT version of original frawk only supports automatic parallelization currently. (was tested on Linux machine and WSL2.0 Ubuntu)
```bash
git clone https://github.com/ikrvc/parallel_frawk.git
cd parallel_frawk

cargo build --no-default-features --features "allow_avx2,use_jemalloc"
```

### Running
Running in a classical sequential mode:
```bash
frawk 'AWK_PROGRAM' input.txt
```
Running in automatic parallel mode (For programs detected as parallelizable, the interpreter automatically uses the parallel execution engine. Otherwise, execution falls back to the standard sequential mode.):
```bash
frawk 'AWK_PROGRAM' input.txt -p a -j {amount of parrallel workers (e.g. 10)}
```
Check if the script can be automatically parallelized (will not actually execute the script, but only check for parallelization):
```bash
frawk 'AWK_PROGRAM' input.txt -p a -j 5 --check-parallel
```

# Evaluation files

The `parallel_evaluation/` directory contains the benchmarking and validation framework used to evaluate the parallelization approach developed in the thesis.

## Purpose

The evaluation framework is used to:

- Compare sequential and parallel execution results.
- Measure execution-time improvements.
- Analyze parallelizability of real-world AWK programs.

## Directory Overview

### `parallel_evaluation/benchmarking_scripts`

Stores scripts used to perform benchmarking

### `parallel_evaluation/benchmarking_results`

Stores generated benchmark measurements

### `parallel_evaluation/benchamrking.py`

Benchmarking script used to test the performance of different AWK versions.

### `parallel_evaluation/applicability_files`

Stores scripts used to extract AWK programs from GitHub and evaluate parallelizability of them


## Thesis Abstract

The project presents the design and implementation of a system for automatic parallelization of AWK programs. AWK remains a widely used language for text processing and data transformation. It is included as a standard utility tool on most Unix-like systems. The execution model of AWK is traditionally sequential, which limits scalability on modern multi-core hardware. The goal of this work is to investigate whether static program analysis can identify AWK scripts that can be executed in parallel and to integrate this capability into an AWK interpreter.

The proposed solution introduces a static analyzer that evaluates AWK programs based on variable dependencies, control flow, and other behaviors that impact data dependencies. The analyzer identifies reduction patterns for global variables and determines whether program semantics can be preserved under parallel execution. These results are then integrated into the interpreter, which enables deterministic multi-threaded execution.

The project adopts the MapReduce programming model to enable parallel execution of AWK. The main processing phase of a script is treated as the map stage, where independent partitions of the input are processed concurrently by multiple workers. Intermediate thread-local results are then combined in a reduce stage using aggregation strategies derived from static analysis. This model provides a structured way to preserve AWK’s sequential semantics in the parallelized environment.

The implementation was evaluated on a dataset of real-world AWK scripts and through performance benchmarks on large text-processing workloads. The results show that a significant subset of AWK programs can be parallelized automatically, achieving execution speedups and state-of-the-art AWK performance.

The project provides a practical path for improving efficiency in text-processing workflows. This work also demonstrates that scripting languages can often benefit from modern parallel execution techniques, extending their practical relevance and performance in data-processing tasks.


# frawk (original README):

*Note (2024, ezrosent@) While the [policy](https://github.com/ezrosent/frawk?tab=readme-ov-file#bugs-and-feature-requests)
on bugs and feature requests remains unchanged I've had much less time over the last 1-2 years to devote to bug fixes and
feature requests for frawk. Other awks are more actively maintained, and CSV support is now a much
more common feature in awk compared to when this project started; I'll update this notice if frawk's status changes.*

frawk is a small programming language for writing short programs processing
textual data. To a first approximation, it is an implementation of the
[AWK](https://en.wikipedia.org/wiki/AWK) language; many common Awk programs
produce equivalent output when passed to frawk. You might be interested in frawk
if you want your scripts to handle escaped CSV/TSV like standard Awk fields, or
if you want your scripts to execute faster.

The info subdirectory has more in-depth information on frawk:

* [Overview](https://github.com/ezrosent/frawk/blob/master/info/overview.md):
  what frawk is all about, how it differs from Awk.
* [Types](https://github.com/ezrosent/frawk/blob/master/info/types.md): A
  quick gloss on frawk's approach to types and type inference.
* [Parallelism](https://github.com/ezrosent/frawk/blob/master/info/parallelism.md):
  An overview of frawk's parallelism support.
* [Benchmarks](https://github.com/ezrosent/frawk/blob/master/info/performance.md):
  A sense of the relative performance of frawk and other tools when processing
  large CSV or TSV files.
* [Builtin Functions Reference](https://github.com/ezrosent/frawk/blob/master/info/reference.md):
  A list of builtin functions implemented by frawk, including some that are new
  when compared with Awk.

frawk is dual-licensed under MIT or Apache 2.0.

## Installation

*Note: frawk uses some nightly-only Rust features by default.
Build [without the `unstable`](https://github.com/ezrosent/frawk#building-using-stable)
feature to build on stable.*  

You will need to [install Rust](https://rustup.rs/). If you have not updated rust in a while, 
run `rustup update nightly` (or `rustup update` if building using stable). If you would like
to use the LLVM backend, you will need an installation of LLVM 12 on your machine: 

* See [this site](https://apt.llvm.org/) for installation instructions on some debian-based Linux distros.
  See also the comments on [this issue](https://github.com/ezrosent/frawk/issues/63) for docker files that
  can be used to build a binary on Ubuntu.
* On Arch `pacman -Sy llvm llvm-libs` and a C compiler (e.g. `clang`) are sufficient as of September 2020.
* `brew install llvm@12` or similar seem to work on Mac OS.

Depending on where your package manager puts these libraries, you may need to
point `LLVM_SYS_120_PREFIX` at the llvm library installation (e.g.
`/usr/lib/llvm-12` on Linux or `/usr/local/opt/llvm@12` on Mac OS when installing llvm@12 via Homebrew).

### Building Without LLVM

While the LLVM backend is recommended, it is possible to build frawk only with
support for the Cranelift-based JIT and its bytecode interpreter. To do this,
build without the `llvm_backend` feature. The Cranelift backend provides
comparable performance to LLVM for smaller scripts, but LLVM's optimizations
can sometimes deliver a substantial performance boost over Cranelift (see the
[benchmarks](https://github.com/ezrosent/frawk/blob/master/info/performance.md)
document for some examples of this).

### Building Using Stable

frawk currently requires a nightly compiler by default. To compile frawk using stable,
compile without the `unstable` feature. Using `rustup default nightly`, or some other
method to run a nightly compiler release is otherwise required to build frawk.

### Building a Binary

With those prerequisites, cloning this repository and a `cargo build --release`
or `cargo [+nightly] install --path <frawk repo path>` will produce a binary that you can
add to your `PATH` if you so choose:

```
$ cd <frawk repo path>
# With LLVM
$ cargo +nightly install --path .
# Without LLVM, but with other recommended defaults
$ cargo +nightly install --path . --no-default-features --features use_jemalloc,allow_avx2,unstable
```

frawk is now on [crates.io](https://crates.io/crates/frawk), so running 
`cargo +nightly install frawk` with the desired features should also work.

While there are no _deliberate_ unix-isms in frawk, I have not tested it on Windows.
frawk does appear to build on Windows with default features disabled; see comments on [this issue](https://github.com/ezrosent/frawk/issues/87)
for more information.

## Bugs and Feature Requests

frawk has bugs, and many rough edges. If you notice a bug in frawk, filing an issue
with an explanation of how to reproduce the error would be very helpful. There are
no guarantees on response time or latency for a fix. No one works on frawk full-time.
The same policy holds for feature requests.
